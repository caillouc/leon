//! Claude Code over stream-json: `claude -p --input-format stream-json
//! --output-format stream-json` (checked on 2.1.288, 2026-11-17).
//!
//! One process per conversation, kept between turns, under a holder that
//! outlives Leon (I24). Stopping a turn ends the process; the next message
//! starts it again with `--resume`.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::Path;

use leon_core::ports::{BackendChannel, BackendCommand, BackendEvent, PortError, SessionSpec};
use leon_domain::{Capability, PlanStep, StepStatus, Usage};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::conn::{Holders, Input, Launch, Opening, Output, ended};
use crate::process::clip;
use crate::summary::{OUTPUT_MAX, claude_files, claude_purpose, claude_tool, result_text};

/// The command line; `skills` is the folder of Leon's generic skills made
/// for Claude Code (EXT-5), if any.
pub(crate) fn args(spec: &SessionSpec, skills: Option<&Path>) -> Vec<String> {
    let s = &spec.settings;
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--prompt-suggestions",
        "true",
        // Questions and approval requests come to Leon (CONV-9).
        "--permission-prompt-tool",
        "stdio",
        "--permission-mode",
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .collect();
    args.push(s.permission.clone());
    if let Some(model) = &s.model {
        args.extend(["--model".to_owned(), model.clone()]);
    }
    if let Some(effort) = &s.effort {
        args.extend(["--effort".to_owned(), effort.clone()]);
    }
    if let Some(agent) = &spec.agent {
        let def = json!({ (agent.name.clone()): { "description": agent.description, "prompt": agent.instructions } });
        args.extend([
            "--agents".to_owned(),
            def.to_string(),
            "--agent".to_owned(),
            agent.name.clone(),
        ]);
    }
    if !spec.mcp_servers.is_empty() {
        args.extend([
            "--mcp-config".to_owned(),
            json!({ "mcpServers": crate::mcp::servers(&spec.mcp_servers) }).to_string(),
        ]);
        // Leon's own tools (P10) are allowed without asking (I6).
        let allowed: Vec<String> = spec
            .mcp_servers
            .iter()
            .filter(|m| m.trusted)
            .map(|m| format!("mcp__{}", m.name))
            .collect();
        if !allowed.is_empty() {
            args.extend(["--allowedTools".to_owned(), allowed.join(",")]);
        }
    }
    // Source folders in the plugin layout load natively (EXT-2, P3).
    for source in &spec.sources {
        args.extend([
            "--plugin-dir".to_owned(),
            source.dir.to_string_lossy().into_owned(),
        ]);
    }
    // What the agent file's `tools` leaves out (EXT-7).
    let denied = spec
        .agent
        .as_ref()
        .and_then(|a| a.tools.as_deref())
        .map_or_default(denied_tools);
    if !denied.is_empty() {
        args.push(format!("--disallowedTools={}", denied.join(",")));
    }
    // Leon's rules for its tools (CTX-3) and `~/.agents/AGENTS.md` (EXT-8),
    // for every agent, after its own prompt.
    if let Some(text) = crate::told::told([
        spec.app_instructions,
        spec.instructions.as_ref().map(|i| i.text.as_str()),
    ]) {
        args.push(format!("--append-system-prompt={text}"));
    }
    // Its `.claude/skills/` loads like a project's (docs, skills page;
    // checked on 2.1.288, 2026-11-18). One value: the flag takes several.
    if let Some(dir) = skills {
        args.push(format!("--add-dir={}", dir.display()));
    }
    if let Some(id) = &spec.resume {
        args.extend(["--resume".to_owned(), id.clone()]);
    }
    args
}

/// Claude Code's tools for the capabilities an agent does not have, by
/// GitHub's alias table (custom-agents reference, 2026-11-18); task tools as
/// 2.1.288 names them.
fn denied_tools(allowed: &[Capability]) -> Vec<&'static str> {
    Capability::ALL
        .into_iter()
        .filter(|c| !allowed.contains(c))
        .flat_map(|c| match c {
            Capability::Read => &["Read"][..],
            Capability::Search => &["Grep", "Glob"],
            Capability::Edit => &["Edit", "Write", "NotebookEdit"],
            Capability::Execute => &["Bash"],
            Capability::Web => &["WebFetch", "WebSearch"],
            Capability::Agent => &["Agent"],
            Capability::Todo => &["TodoWrite", "TaskCreate", "TaskUpdate"],
        })
        .copied()
        .collect()
}

/// Starts Claude Code for `spec`, under a holder when `holders` is set
/// (I24); `skills` is the folder of Leon's generic skills made for Claude
/// Code (EXT-5), if any.
pub(crate) fn start(
    spec: &SessionSpec,
    path: Option<&OsString>,
    skills: Option<&Path>,
    holders: Option<&Holders>,
) -> Result<BackendChannel, PortError> {
    let launch = Launch {
        path,
        program: "claude",
        args: &args(spec, skills),
        env: &[],
        cwd: &spec.working_dir,
    };
    let opening = crate::conn::open(holders, &spec.conversation.0.to_string(), &launch)
        .map_err(|e| PortError::new("starting Claude Code", e))?;
    Ok(session(opening))
}

/// Runs Claude Code's stream-json on the CLI `opening` started or found
/// again (I24).
pub(crate) fn session(opening: Opening) -> BackendChannel {
    let (commands, command_rx) = unbounded_channel();
    let (event_tx, events) = unbounded_channel();
    tokio::spawn(async move {
        let (mut input, mut output) = match opening.connect().await {
            Ok(conn) => conn,
            Err(error) => {
                let _ = event_tx.send(BackendEvent::Failed(format!("Claude Code: {error}")));
                return;
            }
        };
        let outcome = run(&mut input, &mut output, command_rx, &event_tx).await;
        if outcome == Outcome::Stopped {
            input.stop().await;
            return;
        }
        let (ok, tail) = ended(input, output).await;
        if !ok {
            let message = if tail.is_empty() {
                "Claude Code stopped".to_owned()
            } else {
                tail
            };
            let _ = event_tx.send(BackendEvent::Failed(message));
        }
    });
    BackendChannel { commands, events }
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The CLI closed its output.
    Exited,
    /// Leon stopped it (closed the channel or interrupted).
    Stopped,
}

async fn run(
    input: &mut Input,
    output: &mut Output,
    mut commands: UnboundedReceiver<BackendCommand>,
    events: &UnboundedSender<BackendEvent>,
) -> Outcome {
    let mut parser = Parser::default();
    loop {
        tokio::select! {
            line = output.next_line() => match line {
                Some(line) => {
                    if !forward(events, parser.parse(&line)) {
                        return Outcome::Stopped;
                    }
                }
                None => return Outcome::Exited,
            },
            command = commands.recv() => {
                let (new, line) = match command {
                    // A message typed while a card shows answers it.
                    Some(BackendCommand::Send(prompt)) if parser.asks.waiting() => {
                        parser.asks.answer(None, &prompt.text)
                    }
                    Some(BackendCommand::Send(prompt)) => (Vec::new(), Some(user_message(&prompt).to_string())),
                    Some(BackendCommand::Answer { id, answer }) => parser.asks.answer(Some(&id), &answer),
                    Some(BackendCommand::Interrupt) => {
                        let _ = events.send(BackendEvent::TurnEnded { stopped: true, usage: None });
                        return Outcome::Stopped;
                    }
                    None => return Outcome::Stopped,
                };
                if !forward(events, new) {
                    return Outcome::Stopped;
                }
                if let Some(line) = line
                    && !input.send(line.trim_end()).await
                {
                    return Outcome::Exited;
                }
            }
        }
    }
}

/// Sends `new` on to the core; `false` once it no longer listens.
fn forward(events: &UnboundedSender<BackendEvent>, new: Vec<BackendEvent>) -> bool {
    new.into_iter().all(|event| events.send(event).is_ok())
}

/// Turns stream-json lines into events. Subagent output (lines with a
/// `parent_tool_use_id`) only feeds the subagent counts (CONV-13).
#[derive(Debug, Default)]
pub(crate) struct Parser {
    /// The question or approval request Claude Code waits on (CONV-9).
    pub(crate) asks: crate::claude_ask::Asks,
    subagents_running: HashSet<String>,
    subagents_started: u32,
    /// The agent's task list, by task id, in creation order (CONV-14).
    tasks: Vec<(String, PlanStep)>,
    /// It runs on the user's subscription, not an API key: its cost is
    /// only what the API would bill, not shown (USE-1).
    subscription: bool,
}

impl Parser {
    pub(crate) fn parse(&mut self, line: &str) -> Vec<BackendEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let top_level = v.get("parent_tool_use_id").is_none_or(Value::is_null);
        match v["type"].as_str() {
            Some("system") if v["subtype"] == "init" => {
                // "none" on a claude.ai login (2.1.288, a real run on a team
                // subscription, 2026-11-24).
                self.subscription = v["apiKeySource"] == "none";
                v["session_id"]
                    .as_str()
                    .map(|id| BackendEvent::Ready {
                        cli_session: id.to_owned(),
                    })
                    .into_iter()
                    .collect()
            }
            Some("stream_event") if top_level => {
                let event = &v["event"];
                let delta = &event["delta"];
                if event["type"] == "content_block_delta" && delta["type"] == "text_delta" {
                    delta["text"]
                        .as_str()
                        .map(|t| BackendEvent::TextDelta(t.to_owned()))
                        .into_iter()
                        .collect()
                } else {
                    Vec::new()
                }
            }
            // Shape read in Claude Code 2.1.288's own code, 2026-11-17.
            Some("prompt_suggestion") => v["suggestion"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .map(|s| BackendEvent::Suggestion(s.trim().to_owned()))
                .into_iter()
                .collect(),
            // How much of the plan is used (DOCTOR-2); seen on a real run of
            // Claude Code 2.1.288, 2026-11-20.
            Some("rate_limit_event") => crate::limits::claude(&v["rate_limit_info"])
                .map(|windows| BackendEvent::Limits {
                    windows,
                    credits: None,
                })
                .into_iter()
                .collect(),
            Some("control_request") => self.asks.request(&v),
            Some("assistant") if top_level => self.assistant(&v),
            Some("user") if top_level => self.tool_results(&v),
            // A result of Claude Code's own, not of the user's message: on
            // resume, 2.1.288 first reports a background shell of the
            // previous session and closes that report with a result
            // (`origin.kind`: "task-notification", seen 2026-11-23). The
            // user's turn goes on.
            Some("result") if v["origin"]["kind"] == "task-notification" => Vec::new(),
            Some("result") => {
                let mut out = Vec::new();
                if v["is_error"] == true && v["subtype"] != "success" {
                    let text = v["result"]
                        .as_str()
                        .unwrap_or("Claude Code reported an error");
                    out.push(BackendEvent::Failed(text.to_owned()));
                }
                let usage = usage(&v).map(|u| Usage {
                    cost_usd: u.cost_usd.filter(|_| !self.subscription),
                    ..u
                });
                out.push(BackendEvent::TurnEnded {
                    stopped: false,
                    usage,
                });
                out
            }
            _ => Vec::new(),
        }
    }

    fn assistant(&mut self, v: &Value) -> Vec<BackendEvent> {
        let blocks = v["message"]["content"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        if v.get("error").is_some_and(|e| !e.is_null()) {
            let text = blocks
                .iter()
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return vec![BackendEvent::Failed(text)];
        }
        let mut out = Vec::new();
        for block in blocks {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str().filter(|t| !t.trim().is_empty()) {
                        out.push(BackendEvent::Text(text.to_owned()));
                    }
                }
                Some("tool_use") => {
                    let id = block["id"].as_str().unwrap_or_default().to_owned();
                    let name = block["name"].as_str().unwrap_or_default();
                    if matches!(name, "Task" | "Agent") {
                        self.subagents_started += 1;
                        self.subagents_running.insert(id.clone());
                        out.push(self.subagents());
                    }
                    let (verb, detail) = claude_tool(name, &block["input"]);
                    out.push(BackendEvent::Tool {
                        id,
                        name: verb,
                        detail,
                        purpose: claude_purpose(name, &block["input"]),
                    });
                    out.extend(claude_files(name, &block["input"]));
                }
                _ => {}
            }
        }
        out
    }

    fn tool_results(&mut self, v: &Value) -> Vec<BackendEvent> {
        let blocks = v["message"]["content"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let mut out = Vec::new();
        for block in blocks.iter().filter(|b| b["type"] == "tool_result") {
            let id = block["tool_use_id"].as_str().unwrap_or_default().to_owned();
            let output = clip(&result_text(&block["content"]), OUTPUT_MAX);
            let failed = block["is_error"] == true;
            let finished_subagent = self.subagents_running.remove(&id);
            out.push(BackendEvent::ToolDone { id, output, failed });
            if finished_subagent {
                out.push(self.subagents());
            }
        }
        if self.task_change(&v["tool_use_result"]) {
            out.push(BackendEvent::Plan(
                self.tasks.iter().map(|(_, step)| step.clone()).collect(),
            ));
        }
        out
    }

    /// Follows the agent's task list through its task tools' results, as
    /// Claude Code 2.1.288 reports them (seen in a real run, 2026-11-18):
    /// `{"task": {"id", "subject"}}` when one is created, `{"taskId",
    /// "statusChange": {"to"}}` when one moves. Whether it changed.
    fn task_change(&mut self, result: &Value) -> bool {
        if let (Some(id), Some(subject)) = (
            result["task"]["id"].as_str(),
            result["task"]["subject"].as_str(),
        ) {
            self.tasks
                .push((id.to_owned(), PlanStep::new(subject, StepStatus::Pending)));
            return true;
        }
        let (Some(id), Some(to)) = (
            result["taskId"].as_str(),
            result["statusChange"]["to"].as_str(),
        ) else {
            return false;
        };
        if to == "deleted" {
            self.tasks.retain(|(t, _)| t != id);
            return true;
        }
        let status = match to {
            "in_progress" => StepStatus::InProgress,
            "completed" => StepStatus::Done,
            _ => StepStatus::Pending,
        };
        match self.tasks.iter_mut().find(|(t, _)| t == id) {
            Some((_, step)) => {
                step.status = status;
                true
            }
            None => false,
        }
    }

    fn subagents(&self) -> BackendEvent {
        BackendEvent::Subagents {
            started: self.subagents_started,
            running: u32::try_from(self.subagents_running.len()).unwrap_or(u32::MAX),
        }
    }
}

fn usage(v: &Value) -> Option<Usage> {
    let u = v.get("usage")?;
    let n = |key: &str| u.get(key).and_then(Value::as_u64).unwrap_or(0);
    Some(Usage {
        input_tokens: n("input_tokens")
            + n("cache_read_input_tokens")
            + n("cache_creation_input_tokens"),
        output_tokens: n("output_tokens"),
        cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
    })
}

/// A user message in stream-json: its text, after its images as base64
/// `image` blocks (CONV-18; Claude Code 2.1.288, a real run, 2026-11-24).
fn user_message(prompt: &leon_core::ports::Prompt) -> Value {
    let images: Vec<Value> = prompt
        .images
        .iter()
        .filter_map(|path| crate::images::encoded(path))
        .map(|(mime, data)| {
            json!({ "type": "image", "source": { "type": "base64", "media_type": mime, "data": data } })
        })
        .collect();
    let content = if images.is_empty() {
        json!(prompt.text)
    } else {
        let mut blocks = images;
        blocks.push(json!({ "type": "text", "text": prompt.text }));
        Value::Array(blocks)
    };
    json!({ "type": "user", "message": { "role": "user", "content": content } })
}

#[cfg(test)]
mod tests {
    /// A message with images: base64 `image` blocks, then the text; without,
    /// the text alone (CONV-18).
    #[test]
    fn images_go_as_base64_blocks_before_the_text() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("1.png");
        std::fs::write(&image, b"png bytes").unwrap();
        let with = super::user_message(&leon_core::ports::Prompt {
            text: "What is [Image #1]?".into(),
            images: vec![image],
        });
        assert_eq!(
            with["message"]["content"],
            serde_json::json!([
                { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "cG5nIGJ5dGVz" } },
                { "type": "text", "text": "What is [Image #1]?" }
            ])
        );
        let without = super::user_message(&"hi".into());
        assert_eq!(without["message"]["content"], "hi");
    }

    use std::path::PathBuf;

    use leon_core::ports::{AgentDefinition, BackendEvent, SessionSpec};
    use leon_domain::{BackendKind, ConversationSettings, PlanStep, StepStatus};
    use serde_json::{Value, json};

    use super::{Parser, args};

    fn parse_all(lines: &str) -> Vec<BackendEvent> {
        let mut parser = Parser::default();
        lines.lines().flat_map(|l| parser.parse(l)).collect()
    }

    /// Recorded from Claude Code 2.1.288 (login expired, 2026-11-17): the CLI
    /// reports the failure as an assistant message with an `error` field.
    #[test]
    fn recorded_auth_failure() {
        let events = parse_all(include_str!(
            "../fixtures/claude/2.1.288/auth-failure.jsonl"
        ));
        assert!(
            matches!(&events[0], BackendEvent::Ready { cli_session } if !cli_session.is_empty())
        );
        assert!(matches!(&events[1], BackendEvent::Failed(text) if text.contains("authenticate")));
        assert!(matches!(
            events.last(),
            Some(BackendEvent::TurnEnded { stopped: false, .. })
        ));
    }

    /// Recorded from Claude Code 2.1.288 resumed on a conversation, 2026-11-23
    /// (its first four lines; the turn after them in the documented shapes):
    /// it first reports a background shell of the previous session, with a
    /// `result` of its own (`origin: task-notification`), then answers the
    /// user. Only the user's turn ends: the first result ends nothing.
    #[test]
    fn a_task_notification_on_resume_ends_no_turn() {
        let events = parse_all(include_str!(
            "../fixtures/claude/2.1.288/resume-task-notification.jsonl"
        ));
        let ended: Vec<&BackendEvent> = events
            .iter()
            .filter(|e| matches!(e, BackendEvent::TurnEnded { .. }))
            .collect();
        assert_eq!(ended.len(), 1, "{events:?}");
        assert!(matches!(
            events.last(),
            Some(BackendEvent::TurnEnded { .. })
        ));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, BackendEvent::Text(t) if t == "Looking at the code."))
        );
    }

    /// On a subscription Claude Code still says what the API would cost:
    /// Leon leaves it out; with an API key it shows (USE-1).
    #[test]
    fn the_cost_shows_only_with_an_api_key() {
        let turn = |source: &str| {
            let lines = [
                format!(r#"{{"type":"system","subtype":"init","session_id":"s1","apiKeySource":"{source}"}}"#),
                r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.016211,"usage":{"input_tokens":10,"output_tokens":3}}"#.to_owned(),
            ];
            parse_all(&lines.join("\n"))
                .into_iter()
                .find_map(|e| match e {
                    BackendEvent::TurnEnded { usage, .. } => usage.and_then(|u| u.cost_usd),
                    _ => None,
                })
        };
        assert_eq!(turn("none"), None);
        assert_eq!(turn("ANTHROPIC_API_KEY"), Some(0.016_211));
    }

    /// Documented stream-json shapes (⚠️ not recorded yet: needs a logged-in
    /// CLI): text deltas, a tool call and its result, a subagent, the result.
    #[test]
    fn documented_turn() {
        let lines = [
            r#"{"type":"system","subtype":"init","session_id":"s1"}"#,
            r#"{"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hel"}}}"#,
            r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"Hello"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"user","parent_tool_use_id":null,"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"a.rs","is_error":false}]}}"#,
            r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"tool_use","id":"t2","name":"Task","input":{"description":"explore"}}]}}"#,
            r#"{"type":"assistant","parent_tool_use_id":"t2","message":{"content":[{"type":"text","text":"inside the subagent"}]}}"#,
            r#"{"type":"user","parent_tool_use_id":null,"message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":[{"type":"text","text":"done"}]}]}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.01,"usage":{"input_tokens":10,"cache_read_input_tokens":5,"output_tokens":3}}"#,
        ];
        let events = parse_all(&lines.join("\n"));
        let expected = [
            BackendEvent::Ready {
                cli_session: "s1".into(),
            },
            BackendEvent::TextDelta("Hel".into()),
            BackendEvent::Text("Hello".into()),
            BackendEvent::Tool {
                id: "t1".into(),
                name: "Ran".into(),
                detail: "ls".into(),
                purpose: None,
            },
            BackendEvent::ToolDone {
                id: "t1".into(),
                output: "a.rs".into(),
                failed: false,
            },
            BackendEvent::Subagents {
                started: 1,
                running: 1,
            },
            BackendEvent::Tool {
                id: "t2".into(),
                name: "Started a subagent".into(),
                detail: "explore".into(),
                purpose: None,
            },
            BackendEvent::ToolDone {
                id: "t2".into(),
                output: "done".into(),
                failed: false,
            },
            BackendEvent::Subagents {
                started: 1,
                running: 0,
            },
            BackendEvent::TurnEnded {
                stopped: false,
                usage: Some(leon_domain::Usage {
                    input_tokens: 15,
                    output_tokens: 3,
                    cost_usd: Some(0.01),
                }),
            },
        ];
        assert_eq!(events, expected);
    }

    #[test]
    fn garbage_lines_are_ignored() {
        assert_eq!(parse_all("not json\n{\"type\":\"unknown\"}"), Vec::new());
    }

    #[test]
    fn args_pass_every_choice_explicitly() {
        let spec = SessionSpec {
            conversation: leon_domain::ConversationId(1),
            settings: ConversationSettings {
                backend: BackendKind::ClaudeCode,
                agent: Some("planner".into()),
                model: Some("opus".into()),
                effort: Some("high".into()),
                permission: "auto".into(),
            },
            agent: Some(AgentDefinition {
                name: "planner".into(),
                description: "Plans".into(),
                instructions: "Plan.".into(),
                ..Default::default()
            }),
            working_dir: PathBuf::from("/tmp"),
            resume: Some("s1".into()),
            mcp_servers: vec![leon_domain::McpServer {
                name: "leon".into(),
                command: PathBuf::from("/bin/leon"),
                args: vec!["mcp".into()],
                env: vec![],
                trusted: true,
            }],
            sources: vec![leon_core::ports::Source {
                dir: PathBuf::from("/kit"),
                mcp_servers: Vec::new(),
            }],
            skills: Vec::new(),
            instructions: None,
            app_instructions: None,
        };
        let list = args(&spec, None);
        assert!(list.join(" ").contains("--plugin-dir /kit --resume s1"));
        let at = list.iter().position(|a| a == "--mcp-config").unwrap();
        let config: Value = serde_json::from_str(&list[at + 1]).unwrap();
        assert_eq!(
            config,
            json!({ "mcpServers": { "leon": { "command": "/bin/leon", "args": ["mcp"], "env": {} } } })
        );
        assert_eq!(list[at + 2..at + 4], ["--allowedTools", "mcp__leon"]);
        let a = list.join(" ");
        assert!(a.contains("--permission-mode auto"));
        // Questions and approvals come to Leon (CONV-9).
        assert!(a.contains("--permission-prompt-tool stdio"));
        assert!(a.contains("--model opus"));
        assert!(a.contains("--effort high"));
        assert!(a.contains("--agent planner"));
        assert!(a.contains(r#"--agents {"planner":{"description":"Plans","prompt":"Plan."}}"#));
        assert!(a.ends_with("--resume s1"));
        // An agent that may only read and search (EXT-7).
        let mut limited = spec.clone();
        if let Some(agent) = limited.agent.as_mut() {
            agent.tools = Some(vec![
                leon_domain::Capability::Read,
                leon_domain::Capability::Search,
            ]);
        }
        assert!(args(&limited, None).contains(
            &"--disallowedTools=Edit,Write,NotebookEdit,Bash,WebFetch,WebSearch,Agent,TodoWrite,TaskCreate,TaskUpdate"
                .to_owned()
        ));
        assert!(!a.contains("--disallowedTools"));
        // Shared instructions (EXT-8), as one value.
        let mut shared = spec.clone();
        shared.instructions = Some(leon_core::ports::SharedInstructions {
            file: PathBuf::from("/home/u/.agents/AGENTS.md"),
            text: "Be brief.".into(),
        });
        assert!(args(&shared, None).contains(&"--append-system-prompt=Be brief.".to_owned()));
        // Leon's rules for its tools come first (CTX-3).
        shared.app_instructions = Some("Use Leon.");
        assert!(
            args(&shared, None)
                .contains(&"--append-system-prompt=Use Leon.\n\nBe brief.".to_owned())
        );
        // Leon's generic skills folder (EXT-5), as one value.
        let with_skills = args(&spec, Some(std::path::Path::new("/data/skills/claude/ab")));
        assert!(
            with_skills
                .join(" ")
                .contains("--add-dir=/data/skills/claude/ab --resume s1")
        );
    }

    #[test]
    fn prompt_suggestions_become_suggestions() {
        let events = parse_all(
            r#"{"type":"prompt_suggestion","suggestion":"Run the tests","uuid":"u","session_id":"s"}
{"type":"prompt_suggestion","suggestion":"  "}"#,
        );
        assert_eq!(
            events,
            vec![BackendEvent::Suggestion("Run the tests".into())]
        );
    }

    /// The tool results of Claude Code 2.1.288's task tools, as seen in a
    /// real run on 2026-11-18 (three tasks created, two moved).
    #[test]
    fn task_tools_make_the_plan() {
        let result = |id: &str, extra: &str| {
            format!(
                r#"{{"type":"user","parent_tool_use_id":null,"message":{{"content":[{{"type":"tool_result","tool_use_id":"{id}","content":"ok"}}]}},"tool_use_result":{extra}}}"#
            )
        };
        let lines = [
            result("t1", r#"{"task":{"id":"1","subject":"alpha"}}"#),
            result("t2", r#"{"task":{"id":"2","subject":"beta"}}"#),
            result("t3", r#"{"task":{"id":"3","subject":"gamma"}}"#),
            result(
                "t4",
                r#"{"success":true,"taskId":"1","updatedFields":["status"],"statusChange":{"from":"pending","to":"completed"}}"#,
            ),
            result(
                "t5",
                r#"{"success":true,"taskId":"2","updatedFields":["status"],"statusChange":{"from":"pending","to":"in_progress"}}"#,
            ),
        ];
        let plans: Vec<Vec<PlanStep>> = parse_all(&lines.join("\n"))
            .into_iter()
            .filter_map(|e| match e {
                BackendEvent::Plan(steps) => Some(steps),
                _ => None,
            })
            .collect();
        assert_eq!(plans.len(), 5);
        assert_eq!(
            plans.last().unwrap(),
            &[
                PlanStep::new("alpha", StepStatus::Done),
                PlanStep::new("beta", StepStatus::InProgress),
                PlanStep::new("gamma", StepStatus::Pending),
            ]
        );
    }
}
