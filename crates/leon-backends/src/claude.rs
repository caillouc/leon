//! Claude Code over stream-json: `claude -p --input-format stream-json
//! --output-format stream-json` (checked on 2.1.288, 2026-11-17).
//!
//! One process per agent session, kept between turns. Stopping a turn kills
//! the process; the next message starts it again with `--resume`.

use std::collections::HashSet;
use std::ffi::OsString;

use leon_core::ports::{BackendChannel, BackendCommand, BackendEvent, PortError, SessionSpec};
use leon_domain::Usage;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::process::{clip, spawn, stderr_tail};
use crate::summary::{claude_tool, result_text};

const OUTPUT_MAX: usize = 8_000;

pub(crate) fn args(spec: &SessionSpec) -> Vec<String> {
    let s = &spec.settings;
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
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
    if let Some(id) = &spec.resume {
        args.extend(["--resume".to_owned(), id.clone()]);
    }
    args
}

pub(crate) fn start(
    spec: &SessionSpec,
    path: Option<&OsString>,
) -> Result<BackendChannel, PortError> {
    let mut child = spawn(path, "claude", &args(spec), &spec.working_dir)
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
        out
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
    use leon_domain::{BackendKind, SessionSettings};

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
            }),
            working_dir: PathBuf::from("/tmp"),
            resume: Some("s1".into()),
        };
        let a = args(&spec).join(" ");
        assert!(a.contains("--permission-mode auto"));
        assert!(a.contains("--model opus"));
        assert!(a.contains("--effort high"));
        assert!(a.contains("--agent planner"));
        assert!(a.contains(r#"--agents {"planner":{"description":"Plans","prompt":"Plan."}}"#));
        assert!(a.ends_with("--resume s1"));
    }
}
