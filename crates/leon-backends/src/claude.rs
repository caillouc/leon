//! Claude Code over stream-json: `claude -p --input-format stream-json
//! --output-format stream-json` (checked on 2.1.288, 2026-11-17).
//!
//! One process per agent session, kept between turns. Stopping a turn kills
//! the process; the next message starts it again with `--resume`.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::Path;

use leon_core::ports::{BackendChannel, BackendCommand, BackendEvent, PortError, SessionSpec};
use leon_domain::{Capability, PlanStep, StepStatus, Usage};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::process::{clip, spawn, stderr_tail};
use crate::summary::{OUTPUT_MAX, claude_tool, result_text};

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
    // `~/.agents/AGENTS.md` for every agent (EXT-8), after its own prompt.
    if let Some(shared) = &spec.instructions {
        args.push(format!("--append-system-prompt={}", shared.text));
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

pub(crate) fn start(
    spec: &SessionSpec,
    path: Option<&OsString>,
    skills: Option<&Path>,
) -> Result<BackendChannel, PortError> {
    let mut child = spawn(path, "claude", &args(spec, skills), &[], &spec.working_dir)
        .map_err(|e| PortError::new("starting Claude Code", e))?;
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(PortError::new("starting Claude Code", "no stdio pipes"));
    };
    let (commands, command_rx) = unbounded_channel();
    let (event_tx, events) = unbounded_channel();
    tokio::spawn(async move {
        let tail = stderr_tail(stderr);
        let outcome = run(stdin, stdout, command_rx, &event_tx).await;
        let _ = child.start_kill();
        let status = child.wait().await.ok();
        let tail = tail.await.unwrap_or_default();
        if outcome == Outcome::Exited && !status.is_some_and(|s| s.success()) {
            let message = if tail.is_empty() {
                "Claude Code stopped".to_owned()
            } else {
                tail
            };
            let _ = event_tx.send(BackendEvent::Failed(message));
        }
    });
    Ok(BackendChannel { commands, events })
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The CLI closed its output.
    Exited,
    /// Leon stopped it (closed the channel or interrupted).
    Stopped,
}

async fn run(
    mut stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    mut commands: UnboundedReceiver<BackendCommand>,
    events: &UnboundedSender<BackendEvent>,
) -> Outcome {
    let mut lines = BufReader::new(stdout).lines();
    let mut parser = Parser::default();
    loop {
        tokio::select! {
            line = lines.next_line() => match line {
                Ok(Some(line)) => {
                    for event in parser.parse(&line) {
                        if events.send(event).is_err() {
                            return Outcome::Stopped;
                        }
                    }
                }
                _ => return Outcome::Exited,
            },
            command = commands.recv() => match command {
                Some(BackendCommand::Send(text)) => {
                    let message = json!({ "type": "user", "message": { "role": "user", "content": text } });
                    let line = format!("{message}\n");
                    if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                        return Outcome::Exited;
                    }
                }
                Some(BackendCommand::Interrupt) => {
                    let _ = events.send(BackendEvent::TurnEnded { stopped: true, usage: None });
                    return Outcome::Stopped;
                }
                // Claude Code's print mode decides permissions itself.
                Some(BackendCommand::Answer { .. }) => {}
                None => return Outcome::Stopped,
            },
        }
    }
}

/// Turns stream-json lines into events. Subagent output (lines with a
/// `parent_tool_use_id`) only feeds the subagent counts (CONV-13).
#[derive(Debug, Default)]
pub(crate) struct Parser {
    subagents_running: HashSet<String>,
    subagents_started: u32,
    /// The agent's task list, by task id, in creation order (CONV-14).
    tasks: Vec<(String, PlanStep)>,
}

impl Parser {
    pub(crate) fn parse(&mut self, line: &str) -> Vec<BackendEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let top_level = v.get("parent_tool_use_id").is_none_or(Value::is_null);
        match v["type"].as_str() {
            Some("system") if v["subtype"] == "init" => v["session_id"]
                .as_str()
                .map(|id| BackendEvent::Ready {
                    cli_session: id.to_owned(),
                })
                .into_iter()
                .collect(),
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
            Some("assistant") if top_level => self.assistant(&v),
            Some("user") if top_level => self.tool_results(&v),
            Some("result") => {
                let mut out = Vec::new();
                if v["is_error"] == true && v["subtype"] != "success" {
                    let text = v["result"]
                        .as_str()
                        .unwrap_or("Claude Code reported an error");
                    out.push(BackendEvent::Failed(text.to_owned()));
                }
                out.push(BackendEvent::TurnEnded {
                    stopped: false,
                    usage: usage(&v),
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
                    });
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use leon_core::ports::{AgentDefinition, BackendEvent, SessionSpec};
    use leon_domain::{BackendKind, PlanStep, SessionSettings, StepStatus};
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
            settings: SessionSettings {
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
