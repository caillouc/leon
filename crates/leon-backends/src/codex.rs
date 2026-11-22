//! Codex over `codex app-server`: JSON-RPC 2.0, one message per line on
//! stdio (checked on 0.160.0, 2026-11-17).
//!
//! [`Session`] is the protocol state machine, free of I/O; [`start`] wires it
//! to the process.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::Path;

use leon_core::ports::{BackendChannel, BackendCommand, BackendEvent, PortError, SessionSpec};
use leon_domain::{Capability, PlanStep, StepStatus, Usage};
use serde_json::{Value, json};

use crate::conn::{Holders, Launch, Opening};
use crate::options::codex_policy;
use crate::process::clip;
use crate::rpc::{self, Protocol};
use crate::summary::{OUTPUT_MAX, compact, files_changed};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Initialize,
    /// Leon's generic skills folder (EXT-5), set before the thread starts.
    SkillRoots,
    Thread,
    Turn,
    Other,
}

/// Codex app-server protocol state.
#[derive(Debug)]
pub(crate) struct Session {
    next_id: i64,
    pending: HashMap<i64, Pending>,
    /// `skills/extraRoots/set` params, when Leon hands skills to Codex.
    skill_roots: Option<Value>,
    thread_request: (String, Value),
    thread: Option<String>,
    turn: Option<String>,
    queued: Vec<String>,
    model: Option<String>,
    effort: Option<String>,
    usage: Option<Usage>,
    approvals: HashMap<String, Value>,
    subagents_running: HashSet<String>,
    subagents_started: u32,
}

impl Session {
    /// A new session and the messages to send first. `skills` is the folder
    /// of Leon's generic skills made for Codex (EXT-5), if any.
    pub(crate) fn new(spec: &SessionSpec, skills: Option<&Path>) -> (Self, Vec<Value>) {
        let s = &spec.settings;
        let (approval, mut sandbox) = codex_policy(&s.permission);
        let tools = spec.agent.as_ref().and_then(|a| a.tools.as_deref());
        // No `edit` (EXT-7): Codex writes files through its shell and patches.
        if tools.is_some_and(|t| !t.contains(&Capability::Edit)) {
            sandbox = "read-only";
        }
        let mut params = json!({
            "cwd": spec.working_dir.to_string_lossy(),
            "approvalPolicy": approval,
            "sandbox": sandbox,
        });
        if let Some(model) = &s.model {
            params["model"] = json!(model);
        }
        // Leon's rules for its tools (CTX-3), `~/.agents/AGENTS.md` (EXT-8),
        // then the agent's own instructions.
        if let Some(text) = crate::told::told([
            spec.app_instructions,
            spec.instructions.as_ref().map(|i| i.text.as_str()),
            spec.agent.as_ref().map(|a| a.instructions.as_str()),
        ]) {
            params["developerInstructions"] = json!(text);
        }
        // The app's own tools (P10) run without asking; the user's servers
        // from their source folders (EXT-2) follow their usual approvals.
        let mut servers = crate::mcp::servers(&spec.mcp_servers);
        if let Some(map) = servers.as_object_mut() {
            for trusted in spec.mcp_servers.iter().filter(|m| m.trusted) {
                if let Some(server) = map.get_mut(&trusted.name) {
                    server["default_tools_approval_mode"] = json!("approve");
                }
            }
            let theirs: Vec<_> = spec
                .sources
                .iter()
                .flat_map(|s| s.mcp_servers.clone())
                .collect();
            if let Some(user) = crate::mcp::servers(&theirs).as_object() {
                for (name, server) in user {
                    map.entry(name.clone()).or_insert_with(|| server.clone());
                }
            }
            if !map.is_empty() {
                params["config"] = json!({ "mcp_servers": servers });
            }
        }
        if let Some(tools) = tools {
            limit(&mut params, tools);
        }
        let method = match &spec.resume {
            Some(id) => {
                params["threadId"] = json!(id);
                "thread/resume"
            }
            None => "thread/start",
        };
        let mut session = Self {
            next_id: 0,
            pending: HashMap::new(),
            // Checked on 0.160.0 (2026-11-18): the folder's skills then
            // show in `skills/list`.
            skill_roots: skills.map(|dir| json!({ "extraRoots": [dir.to_string_lossy()] })),
            thread_request: (method.to_owned(), params),
            thread: None,
            turn: None,
            queued: Vec::new(),
            model: s.model.clone(),
            effort: s.effort.clone(),
            usage: None,
            approvals: HashMap::new(),
            subagents_running: HashSet::new(),
            subagents_started: 0,
        };
        let init = session.request(
            Pending::Initialize,
            "initialize",
            &json!({ "clientInfo": { "name": "leon", "version": env!("CARGO_PKG_VERSION") } }),
        );
        (session, vec![init])
    }

    fn thread_start(&mut self) -> Value {
        let (method, params) = self.thread_request.clone();
        self.request(Pending::Thread, &method, &params)
    }

    fn request(&mut self, kind: Pending, method: &str, params: &Value) -> Value {
        self.next_id += 1;
        self.pending.insert(self.next_id, kind);
        json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params })
    }

    fn turn_start(&mut self, thread: &str, text: &str) -> Value {
        let mut params = json!({ "threadId": thread, "input": [{ "type": "text", "text": text }] });
        if let Some(model) = &self.model {
            params["model"] = json!(model);
        }
        if let Some(effort) = &self.effort {
            params["effort"] = json!(effort);
        }
        self.request(Pending::Turn, "turn/start", &params)
    }

    /// A command from the core; returns the messages to send.
    pub(crate) fn command(&mut self, command: BackendCommand) -> Vec<Value> {
        match command {
            BackendCommand::Send(text) => {
                if let Some(thread) = self.thread.clone() {
                    vec![self.turn_start(&thread, &text)]
                } else {
                    self.queued.push(text);
                    Vec::new()
                }
            }
            BackendCommand::Interrupt => match (self.thread.clone(), self.turn.clone()) {
                (Some(thread), Some(turn)) => {
                    vec![self.request(
                        Pending::Other,
                        "turn/interrupt",
                        &json!({ "threadId": thread, "turnId": turn }),
                    )]
                }
                _ => Vec::new(),
            },
            BackendCommand::Answer { id, answer } => match self.approvals.remove(&id) {
                Some(rpc_id) => {
                    let decision = if answer.starts_with("Allow") {
                        "accept"
                    } else {
                        "decline"
                    };
                    vec![
                        json!({ "jsonrpc": "2.0", "id": rpc_id, "result": { "decision": decision } }),
                    ]
                }
                None => Vec::new(),
            },
        }
    }

    /// A message from the server; returns events and messages to send back.
    pub(crate) fn handle(&mut self, msg: &Value) -> (Vec<BackendEvent>, Vec<Value>) {
        match (msg.get("id"), msg.get("method")) {
            (Some(id), None) => self.response(id, msg),
            (Some(id), Some(method)) => (
                self.server_request(id, method.as_str().unwrap_or_default(), &msg["params"]),
                Vec::new(),
            ),
            (None, Some(method)) => (
                self.notification(method.as_str().unwrap_or_default(), &msg["params"]),
                Vec::new(),
            ),
            (None, None) => (Vec::new(), Vec::new()),
        }
    }

    fn response(&mut self, id: &Value, msg: &Value) -> (Vec<BackendEvent>, Vec<Value>) {
        let kind = id
            .as_i64()
            .and_then(|id| self.pending.remove(&id))
            .unwrap_or(Pending::Other);
        if let Some(error) = msg.get("error") {
            let text = error["message"]
                .as_str()
                .unwrap_or("Codex refused a request")
                .to_owned();
            return (vec![BackendEvent::Failed(text)], Vec::new());
        }
        let result = &msg["result"];
        match kind {
            Pending::Initialize => {
                let initialized = json!({ "jsonrpc": "2.0", "method": "initialized" });
                let next = match self.skill_roots.take() {
                    Some(roots) => {
                        self.request(Pending::SkillRoots, "skills/extraRoots/set", &roots)
                    }
                    None => self.thread_start(),
                };
                (Vec::new(), vec![initialized, next])
            }
            Pending::SkillRoots => (Vec::new(), vec![self.thread_start()]),
            Pending::Thread => {
                let Some(thread) = result["thread"]["id"].as_str().map(str::to_owned) else {
                    return (
                        vec![BackendEvent::Failed("Codex did not return a thread".into())],
                        Vec::new(),
                    );
                };
                self.thread = Some(thread.clone());
                let queued = std::mem::take(&mut self.queued);
                let sends = queued
                    .iter()
                    .map(|text| self.turn_start(&thread, text))
                    .collect();
                (
                    vec![BackendEvent::Ready {
                        cli_session: thread,
                    }],
                    sends,
                )
            }
            Pending::Turn => {
                self.turn = result["turn"]["id"].as_str().map(str::to_owned);
                (Vec::new(), Vec::new())
            }
            Pending::Other => (Vec::new(), Vec::new()),
        }
    }

    fn server_request(&mut self, id: &Value, method: &str, p: &Value) -> Vec<BackendEvent> {
        let (title, detail) = match method {
            "item/commandExecution/requestApproval" | "execCommandApproval" => {
                ("Run this command?", command_text(&p["command"]))
            }
            "item/fileChange/requestApproval" | "applyPatchApproval" => (
                "Apply these file changes?",
                p["reason"].as_str().unwrap_or_default().to_owned(),
            ),
            other => {
                tracing::debug!(method = other, "unsupported Codex request");
                return Vec::new();
            }
        };
        let question = format!("codex-{id}");
        self.approvals.insert(question.clone(), id.clone());
        vec![BackendEvent::Question {
            id: question,
            title: title.to_owned(),
            detail,
            options: vec!["Allow once".to_owned(), "Deny".to_owned()],
        }]
    }

    fn notification(&mut self, method: &str, p: &Value) -> Vec<BackendEvent> {
        match method {
            "item/agentMessage/delta" => p["delta"]
                .as_str()
                .map(|d| BackendEvent::TextDelta(d.to_owned()))
                .into_iter()
                .collect(),
            "item/started" => self.item_started(&p["item"]),
            "item/completed" => self.item_completed(&p["item"]),
            // How much of the plan is used (DOCTOR-2).
            "account/rateLimits/updated" => crate::limits::codex(&p["rateLimits"])
                .map(BackendEvent::Limits)
                .into_iter()
                .collect(),
            "turn/started" => {
                self.turn = p["turn"]["id"].as_str().map(str::to_owned);
                Vec::new()
            }
            // `TurnPlanUpdatedNotification` in the app-server schema Codex
            // 0.160.0 generates: `plan: [{step, status}]`, status one of
            // pending, inProgress, completed (read 2026-11-18).
            "turn/plan/updated" => {
                let steps = p["plan"]
                    .as_array()
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .filter_map(|s| {
                        let status = match s["status"].as_str() {
                            Some("inProgress") => StepStatus::InProgress,
                            Some("completed") => StepStatus::Done,
                            _ => StepStatus::Pending,
                        };
                        Some(PlanStep::new(s["step"].as_str()?, status))
                    })
                    .collect();
                vec![BackendEvent::Plan(steps)]
            }
            "thread/tokenUsage/updated" => {
                let last = &p["tokenUsage"]["last"];
                self.usage = Some(Usage {
                    input_tokens: last["inputTokens"].as_u64().unwrap_or(0),
                    output_tokens: last["outputTokens"].as_u64().unwrap_or(0),
                    cost_usd: None,
                });
                Vec::new()
            }
            "turn/completed" => {
                self.turn = None;
                let turn = &p["turn"];
                let mut out = Vec::new();
                if turn["status"] == "failed" {
                    let text = turn["error"]["message"]
                        .as_str()
                        .unwrap_or("the turn failed");
                    out.push(BackendEvent::Failed(text.to_owned()));
                }
                out.push(BackendEvent::TurnEnded {
                    stopped: turn["status"] == "interrupted",
                    usage: self.usage.take(),
                });
                out
            }
            "error" if p["willRetry"] != true => {
                vec![BackendEvent::Failed(
                    p["error"]["message"]
                        .as_str()
                        .unwrap_or("Codex error")
                        .to_owned(),
                )]
            }
            _ => Vec::new(),
        }
    }

    fn item_started(&mut self, item: &Value) -> Vec<BackendEvent> {
        let id = item["id"].as_str().unwrap_or_default().to_owned();
        let tool = |name: &str, detail: String| {
            vec![BackendEvent::Tool {
                id: id.clone(),
                name: name.to_owned(),
                detail,
            }]
        };
        match item["type"].as_str() {
            Some("commandExecution") => tool("Ran", command_text(&item["command"])),
            Some("fileChange") => {
                let mut out = tool("Edited", changed_paths(&item["changes"]));
                out.extend(files_changed(
                    item["changes"]
                        .as_array()
                        .map_or(&[][..], Vec::as_slice)
                        .iter()
                        .map(|c| &c["path"]),
                ));
                out
            }
            Some("mcpToolCall") => {
                let name = format!(
                    "{}.{}",
                    item["server"].as_str().unwrap_or("mcp"),
                    item["tool"].as_str().unwrap_or("tool")
                );
                vec![BackendEvent::Tool {
                    id,
                    name,
                    detail: compact(&item["arguments"]),
                }]
            }
            Some("webSearch") => tool(
                "Searched the web",
                item["query"].as_str().unwrap_or_default().to_owned(),
            ),
            Some("collabAgentToolCall") => {
                self.subagents_started += 1;
                self.subagents_running.insert(id);
                vec![self.subagents()]
            }
            _ => Vec::new(),
        }
    }

    fn item_completed(&mut self, item: &Value) -> Vec<BackendEvent> {
        let id = item["id"].as_str().unwrap_or_default().to_owned();
        match item["type"].as_str() {
            Some("agentMessage") => item["text"]
                .as_str()
                .filter(|t| !t.trim().is_empty())
                .map(|t| BackendEvent::Text(t.to_owned()))
                .into_iter()
                .collect(),
            Some("commandExecution") => {
                let output = clip(
                    item["aggregatedOutput"].as_str().unwrap_or_default(),
                    OUTPUT_MAX,
                );
                let failed = item["exitCode"].as_i64().is_some_and(|c| c != 0);
                vec![BackendEvent::ToolDone { id, output, failed }]
            }
            Some("fileChange" | "mcpToolCall" | "webSearch") => {
                let failed = item["status"] == "failed" || item["status"] == "declined";
                vec![BackendEvent::ToolDone {
                    id,
                    output: String::new(),
                    failed,
                }]
            }
            Some("collabAgentToolCall") if self.subagents_running.remove(&id) => {
                vec![self.subagents()]
            }
            _ => Vec::new(),
        }
    }

    fn subagents(&self) -> BackendEvent {
        BackendEvent::Subagents {
            started: self.subagents_started,
            running: u32::try_from(self.subagents_running.len()).unwrap_or(u32::MAX),
        }
    }
}

fn command_text(command: &Value) -> String {
    match command {
        Value::String(s) => clip(s, 200),
        Value::Array(parts) => clip(
            &parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
            200,
        ),
        _ => String::new(),
    }
}

fn changed_paths(changes: &Value) -> String {
    let paths: Vec<&str> = changes
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|c| c["path"].as_str())
        .collect();
    clip(&paths.join(", "), 200)
}

/// Turns off what an agent file's `tools` leaves out (EXT-7), in the
/// thread's config (keys from Codex's config reference, 2026-11-18; ⚠️ their
/// effect through `thread/start` not checked in a real turn). Codex reads
/// and searches through its shell and always keeps its plan: `read`,
/// `search` and `todo` cannot be taken away.
fn limit(params: &mut Value, tools: &[Capability]) {
    let mut config = params["config"].take();
    if !config.is_object() {
        config = json!({});
    }
    if !tools.contains(&Capability::Execute) {
        config["features"]["shell_tool"] = json!(false);
    }
    if !tools.contains(&Capability::Web) {
        config["web_search"] = json!("disabled");
    }
    if !tools.contains(&Capability::Agent) {
        config["features"]["multi_agent"] = json!(false);
    }
    if config.as_object().is_some_and(|c| !c.is_empty()) {
        params["config"] = config;
    }
}

/// Starts Codex for `spec`, under a holder when `holders` is set (I24).
pub(crate) fn start(
    spec: &SessionSpec,
    path: Option<&OsString>,
    skills: Option<&Path>,
    holders: Option<&Holders>,
) -> Result<BackendChannel, PortError> {
    let (session, first) = Session::new(spec, skills);
    let launch = Launch {
        path,
        program: "codex",
        args: &["app-server".to_owned()],
        env: &[],
        cwd: &spec.working_dir,
    };
    let opening = crate::conn::open(holders, &spec.conversation.0.to_string(), &launch)
        .map_err(|e| PortError::new("starting a CLI", format!("Codex: {e}")))?;
    Ok(rpc::session(opening, "Codex", session, first))
}

/// Codex found again under its holder (I24): already set up, on its
/// thread; nothing to send first.
pub(crate) fn attach(
    opening: Opening,
    spec: &SessionSpec,
    skills: Option<&Path>,
) -> BackendChannel {
    let (mut session, _) = Session::new(spec, skills);
    session.thread.clone_from(&spec.resume);
    // Far from the ids Codex may still answer from before.
    session.next_id = 1_000_000;
    session.pending.clear();
    rpc::session(opening, "Codex", session, Vec::new())
}

impl Protocol for Session {
    fn handle(&mut self, msg: &Value) -> (Vec<BackendEvent>, Vec<Value>) {
        Session::handle(self, msg)
    }
    fn command(&mut self, command: BackendCommand) -> Vec<Value> {
        Session::command(self, command)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use leon_core::ports::{AgentDefinition, BackendCommand, BackendEvent, SessionSpec};
    use leon_domain::Capability;
    use leon_domain::{BackendKind, ConversationSettings, PlanStep, StepStatus, Usage};
    use serde_json::{Value, json};

    use super::Session;

    fn spec(resume: Option<&str>) -> SessionSpec {
        SessionSpec {
            conversation: leon_domain::ConversationId(1),
            settings: ConversationSettings {
                backend: BackendKind::Codex,
                agent: Some("coder".into()),
                model: Some("gpt-6-luna".into()),
                effort: Some("low".into()),
                permission: "auto".into(),
            },
            agent: Some(AgentDefinition {
                name: "coder".into(),
                description: String::new(),
                instructions: "Write code.".into(),
                ..Default::default()
            }),
            working_dir: PathBuf::from("/tmp/project"),
            resume: resume.map(str::to_owned),
            mcp_servers: Vec::new(),
            sources: Vec::new(),
            skills: Vec::new(),
            instructions: None,
            app_instructions: None,
        }
    }

    #[test]
    fn app_tools_go_in_the_thread_config() {
        let mut with_tools = spec(None);
        with_tools.mcp_servers = vec![leon_domain::McpServer {
            name: "leon".into(),
            command: PathBuf::from("/bin/leon"),
            args: vec!["mcp".into()],
            env: vec![],
            trusted: true,
        }];
        let (session, _) = Session::new(&with_tools, None);
        assert_eq!(
            session.thread_request.1["config"]["mcp_servers"]["leon"]["command"],
            "/bin/leon"
        );
        let (session, _) = Session::new(&spec(None), None);
        assert!(session.thread_request.1.get("config").is_none());
    }

    #[test]
    fn an_agent_without_edit_and_execute_reads_only() {
        let mut limited = spec(None);
        if let Some(agent) = limited.agent.as_mut() {
            agent.tools = Some(vec![Capability::Read, Capability::Search, Capability::Web]);
        }
        let (session, _) = Session::new(&limited, None);
        let params = &session.thread_request.1;
        assert_eq!(params["sandbox"], "read-only");
        assert_eq!(
            params["config"],
            json!({ "features": { "shell_tool": false, "multi_agent": false } })
        );
        // No `tools`: the permission's sandbox, no config.
        let (session, _) = Session::new(&spec(None), None);
        assert_eq!(session.thread_request.1["sandbox"], "workspace-write");
        assert!(session.thread_request.1.get("config").is_none());
    }

    #[test]
    fn the_skills_folder_is_set_before_the_thread_starts() {
        let dir = std::path::Path::new("/data/skills/codex/ab");
        let (mut session, first) = Session::new(&spec(None), Some(dir));
        let (_, sent) = session.handle(&json!({ "id": first[0]["id"], "result": {} }));
        let methods: Vec<_> = sent.iter().map(|m| m["method"].clone()).collect();
        assert_eq!(methods, ["initialized", "skills/extraRoots/set"]);
        assert_eq!(
            sent[1]["params"],
            json!({ "extraRoots": ["/data/skills/codex/ab"] })
        );
        let (_, sent) = session.handle(&json!({ "id": sent[1]["id"], "result": {} }));
        assert_eq!(sent[0]["method"], "thread/start");
    }

    /// Replays the recorded Codex 0.160.0 exchange ("Reply with the single
    /// word: pong"), answering requests in the order the probe sent them.
    #[test]
    fn recorded_pong_turn() {
        let (mut session, first) = Session::new(&spec(None), None);
        assert_eq!(first[0]["method"], "initialize");
        let mut events = Vec::new();
        let mut sent = first;
        for line in include_str!("../fixtures/codex/0.160.0/pong.jsonl").lines() {
            let mut msg: Value = serde_json::from_str(line).unwrap();
            // The probe's ids differ from ours: map responses onto our requests.
            if msg.get("result").is_some() {
                let ours = match msg["id"].as_i64() {
                    Some(1) => 1,
                    Some(3) => 2,
                    Some(4) => 3,
                    _ => continue,
                };
                msg["id"] = json!(ours);
            }
            let (new_events, replies) = session.handle(&msg);
            events.extend(new_events);
            sent.extend(replies);
            if events.len() == 1 && sent.iter().all(|m| m["method"] != "turn/start") {
                sent.extend(session.command(BackendCommand::Send(
                    "Reply with the single word: pong".into(),
                )));
            }
        }
        let methods: Vec<&str> = sent.iter().filter_map(|m| m["method"].as_str()).collect();
        assert_eq!(
            methods,
            ["initialize", "initialized", "thread/start", "turn/start"]
        );
        let thread_start = &sent[2]["params"];
        assert_eq!(thread_start["approvalPolicy"], "never");
        assert_eq!(thread_start["sandbox"], "workspace-write");
        assert_eq!(thread_start["developerInstructions"], "Write code.");
        // `~/.agents/AGENTS.md` comes first (EXT-8).
        let mut shared = spec(None);
        shared.instructions = Some(leon_core::ports::SharedInstructions {
            file: PathBuf::from("/home/u/.agents/AGENTS.md"),
            text: "Be brief.".into(),
        });
        shared.app_instructions = Some("Use Leon.");
        let (session, _) = Session::new(&shared, None);
        assert_eq!(
            session.thread_request.1["developerInstructions"],
            "Use Leon.\n\nBe brief.\n\nWrite code."
        );
        assert_eq!(sent[3]["params"]["effort"], "low");
        assert_eq!(
            events,
            [
                BackendEvent::Ready {
                    cli_session: "01a1eae8-c129-7a31-87d0-bd637eb17aa5".into()
                },
                BackendEvent::TextDelta("pong".into()),
                BackendEvent::Text("pong".into()),
                // Its free plan's 30-day window (DOCTOR-2).
                BackendEvent::Limits(vec![leon_domain::LimitWindow {
                    minutes: 43_200,
                    used: 1,
                    resets_at: Some(1_797_093_720_000),
                }]),
                BackendEvent::TurnEnded {
                    stopped: false,
                    usage: Some(Usage {
                        input_tokens: 15963,
                        output_tokens: 5,
                        cost_usd: None
                    })
                },
            ]
        );
    }

    #[test]
    fn resume_uses_thread_resume() {
        let (mut session, _) = Session::new(&spec(Some("t-1")), None);
        let (_, replies) = session.handle(&json!({ "id": 1, "result": {} }));
        assert_eq!(replies[1]["method"], "thread/resume");
        assert_eq!(replies[1]["params"]["threadId"], "t-1");
    }

    #[test]
    fn messages_before_the_thread_are_queued() {
        let (mut session, _) = Session::new(&spec(None), None);
        assert_eq!(
            session.command(BackendCommand::Send("hi".into())),
            [] as [serde_json::Value; 0]
        );
        session.handle(&json!({ "id": 1, "result": {} }));
        let (events, replies) =
            session.handle(&json!({ "id": 2, "result": { "thread": { "id": "t-9" } } }));
        assert_eq!(
            events,
            [BackendEvent::Ready {
                cli_session: "t-9".into()
            }]
        );
        assert_eq!(replies[0]["method"], "turn/start");
        assert_eq!(replies[0]["params"]["input"][0]["text"], "hi");
    }

    #[test]
    fn approvals_become_questions_and_answers_go_back() {
        let (mut session, _) = Session::new(&spec(None), None);
        let (events, _) = session.handle(&json!({
            "id": 7, "method": "item/commandExecution/requestApproval", "params": { "command": "rm -rf build" }
        }));
        let BackendEvent::Question { id, detail, .. } = &events[0] else {
            panic!("{events:?}")
        };
        assert_eq!(detail, "rm -rf build");
        let replies = session.command(BackendCommand::Answer {
            id: id.clone(),
            answer: "Allow once".into(),
        });
        assert_eq!(
            replies[0],
            json!({ "jsonrpc": "2.0", "id": 7, "result": { "decision": "accept" } })
        );
    }

    #[test]
    fn commands_and_failures_map_to_events() {
        let (mut session, _) = Session::new(&spec(None), None);
        let started = json!({ "method": "item/started", "params": { "item": { "type": "commandExecution", "id": "c1", "command": "cargo test" } } });
        let done = json!({ "method": "item/completed", "params": { "item": { "type": "commandExecution", "id": "c1", "aggregatedOutput": "ok", "exitCode": 1 } } });
        let failed = json!({ "method": "turn/completed", "params": { "turn": { "status": "failed", "error": { "message": "boom" } } } });
        assert_eq!(
            session.handle(&started).0,
            [BackendEvent::Tool {
                id: "c1".into(),
                name: "Ran".into(),
                detail: "cargo test".into()
            }]
        );
        assert_eq!(
            session.handle(&done).0,
            [BackendEvent::ToolDone {
                id: "c1".into(),
                output: "ok".into(),
                failed: true
            }]
        );
        assert_eq!(
            session.handle(&failed).0,
            [
                BackendEvent::Failed("boom".into()),
                BackendEvent::TurnEnded {
                    stopped: false,
                    usage: None
                }
            ]
        );
    }

    /// A file change names its files, so Leon follows the folders an agent
    /// works in (CTX-2).
    #[test]
    fn file_changes_name_their_files() {
        let (mut session, _) = Session::new(&spec(None), None);
        let started = json!({ "method": "item/started", "params": { "item": { "type": "fileChange", "id": "f1",
            "changes": [{ "path": "/r/a.rs" }, { "path": "/r/b.rs" }] } } });
        assert_eq!(
            session.handle(&started).0,
            [
                BackendEvent::Tool {
                    id: "f1".into(),
                    name: "Edited".into(),
                    detail: "/r/a.rs, /r/b.rs".into()
                },
                BackendEvent::FilesChanged(vec![
                    PathBuf::from("/r/a.rs"),
                    PathBuf::from("/r/b.rs")
                ]),
            ]
        );
    }

    #[test]
    fn plan_updates_become_the_plan() {
        let (mut session, _) = Session::new(&spec(None), None);
        let update = json!({ "method": "turn/plan/updated", "params": {
            "threadId": "t", "turnId": "u", "explanation": null,
            "plan": [
                { "step": "Read the retry code", "status": "completed" },
                { "step": "Cap the delay", "status": "inProgress" },
                { "step": "Add a test", "status": "pending" }
            ]
        } });
        assert_eq!(
            session.handle(&update).0,
            [BackendEvent::Plan(vec![
                PlanStep::new("Read the retry code", StepStatus::Done),
                PlanStep::new("Cap the delay", StepStatus::InProgress),
                PlanStep::new("Add a test", StepStatus::Pending),
            ])]
        );
    }
}
