//! Codex over `codex app-server`: JSON-RPC 2.0, one message per line on
//! stdio (checked on 0.160.0, 2026-11-17).
//!
//! [`Session`] is the protocol state machine, free of I/O; [`start`] wires it
//! to the process.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;

use leon_core::ports::{BackendChannel, BackendCommand, BackendEvent, PortError, SessionSpec};
use leon_domain::Usage;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::unbounded_channel;

use crate::options::codex_policy;
use crate::process::{clip, spawn, stderr_tail};
use crate::summary::compact;

const OUTPUT_MAX: usize = 8_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Initialize,
    Thread,
    Turn,
    Other,
}

/// Codex app-server protocol state.
#[derive(Debug)]
pub(crate) struct Session {
    next_id: i64,
    pending: HashMap<i64, Pending>,
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
    /// A new session and the messages to send first.
    pub(crate) fn new(spec: &SessionSpec) -> (Self, Vec<Value>) {
        let s = &spec.settings;
        let (approval, sandbox) = codex_policy(&s.permission);
        let mut params = json!({
            "cwd": spec.working_dir.to_string_lossy(),
            "approvalPolicy": approval,
            "sandbox": sandbox,
        });
        if let Some(model) = &s.model {
            params["model"] = json!(model);
        }
        if let Some(agent) = &spec.agent {
            params["developerInstructions"] = json!(agent.instructions);
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
                let (method, params) = self.thread_request.clone();
                let initialized = json!({ "jsonrpc": "2.0", "method": "initialized" });
                let thread = self.request(Pending::Thread, &method, &params);
                (Vec::new(), vec![initialized, thread])
            }
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
            "turn/started" => {
                self.turn = p["turn"]["id"].as_str().map(str::to_owned);
                Vec::new()
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
            Some("fileChange") => tool("Edited", changed_paths(&item["changes"])),
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

pub(crate) fn start(
    spec: &SessionSpec,
    path: Option<&OsString>,
) -> Result<BackendChannel, PortError> {
    let args = vec!["app-server".to_owned()];
    let mut child = spawn(path, "codex", &args, &spec.working_dir)
        .map_err(|e| PortError::new("starting Codex", e))?;
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(PortError::new("starting Codex", "no stdio pipes"));
    };
    let (session, first) = Session::new(spec);
    let (commands, mut command_rx) = unbounded_channel();
    let (events, event_rx) = unbounded_channel();
    tokio::spawn(async move {
        let tail = stderr_tail(stderr);
        let mut session = session;
        let mut lines = BufReader::new(stdout).lines();
        let mut exited = false;
        let mut outgoing = first;
        loop {
            for msg in outgoing.drain(..) {
                let line = format!("{msg}\n");
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                    exited = true;
                }
            }
            if exited {
                break;
            }
            tokio::select! {
                line = lines.next_line() => if let Ok(Some(line)) = line {
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                    let (new_events, replies) = session.handle(&msg);
                    outgoing = replies;
                    if new_events.into_iter().any(|e| events.send(e).is_err()) {
                        break;
                    }
                } else {
                    exited = true;
                    break;
                },
                command = command_rx.recv() => match command {
                    Some(command) => outgoing = session.command(command),
                    None => break,
                },
            }
        }
        let _ = child.start_kill();
        let _ = child.wait().await;
        let tail = tail.await.unwrap_or_default();
        if exited {
            let message = if tail.is_empty() {
                "Codex stopped".to_owned()
            } else {
                tail
            };
            let _ = events.send(BackendEvent::Failed(message));
        }
    });
    Ok(BackendChannel {
        commands,
        events: event_rx,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use leon_core::ports::{AgentDefinition, BackendCommand, BackendEvent, SessionSpec};
    use leon_domain::{BackendKind, SessionSettings, Usage};
    use serde_json::{Value, json};

    use super::Session;

    fn spec(resume: Option<&str>) -> SessionSpec {
        SessionSpec {
            settings: SessionSettings {
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
            }),
            working_dir: PathBuf::from("/tmp/project"),
            resume: resume.map(str::to_owned),
        }
    }

    /// Replays the recorded Codex 0.160.0 exchange ("Reply with the single
    /// word: pong"), answering requests in the order the probe sent them.
    #[test]
    fn recorded_pong_turn() {
        let (mut session, first) = Session::new(&spec(None));
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
        assert_eq!(sent[3]["params"]["effort"], "low");
        assert_eq!(
            events,
            [
                BackendEvent::Ready {
                    cli_session: "01a1eae8-c129-7a31-87d0-bd637eb17aa5".into()
                },
                BackendEvent::TextDelta("pong".into()),
                BackendEvent::Text("pong".into()),
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
        let (mut session, _) = Session::new(&spec(Some("t-1")));
        let (_, replies) = session.handle(&json!({ "id": 1, "result": {} }));
        assert_eq!(replies[1]["method"], "thread/resume");
        assert_eq!(replies[1]["params"]["threadId"], "t-1");
    }

    #[test]
    fn messages_before_the_thread_are_queued() {
        let (mut session, _) = Session::new(&spec(None));
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
        let (mut session, _) = Session::new(&spec(None));
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
        let (mut session, _) = Session::new(&spec(None));
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
}
