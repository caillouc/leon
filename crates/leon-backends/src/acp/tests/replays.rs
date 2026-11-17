//! The spec's example exchanges, replayed as a table of cases.

use leon_core::ports::{BackendCommand, BackendEvent};

use super::{SESSION, describe, ended, ready, replay, send, spec};

fn read_tool() -> BackendEvent {
    BackendEvent::Tool {
        id: "call_001".into(),
        name: "Read".into(),
        detail: "Reading configuration file".into(),
    }
}

fn read_question() -> BackendEvent {
    BackendEvent::Question {
        id: "acp-5".into(),
        title: "Allow this tool call?".into(),
        detail: "Reading configuration file".into(),
        options: vec!["Allow once".into(), "Reject".into()],
    }
}

struct Case {
    name: &'static str,
    resume: Option<&'static str>,
    lines: &'static str,
    commands: Vec<(usize, BackendCommand)>,
    events: Vec<BackendEvent>,
    sent: &'static [&'static str],
}

#[allow(clippy::too_many_lines, reason = "one table of cases")]
fn cases() -> Vec<Case> {
    let analysis = "Analysis complete:\n- No syntax errors found\n- Consider adding type hints for better clarity\n- The function could benefit from error handling for empty lists";
    let intro = "I'll analyze your code for potential issues. Let me examine it...";
    vec![
        Case {
            name: "a turn: text, a tool call that completes, the end",
            resume: None,
            lines: include_str!("../../../fixtures/acp/1.32.0/turn.jsonl"),
            commands: vec![(0, send("Can you analyze this code?"))],
            events: vec![
                ready(SESSION),
                BackendEvent::TextDelta(intro.into()),
                BackendEvent::Text(intro.into()),
                BackendEvent::Tool {
                    id: "call_001".into(),
                    name: "Analyzing Python code".into(),
                    detail: String::new(),
                },
                BackendEvent::ToolDone {
                    id: "call_001".into(),
                    output: analysis.into(),
                    failed: false,
                },
                ended(false),
            ],
            sent: &["initialize", "session/new", "session/prompt"],
        },
        Case {
            name: "a permission request answered",
            resume: None,
            lines: include_str!("../../../fixtures/acp/1.32.0/permission.jsonl"),
            commands: vec![
                (0, send("Read the config")),
                (
                    4,
                    BackendCommand::Answer {
                        id: "acp-5".into(),
                        answer: "Allow once".into(),
                    },
                ),
            ],
            events: vec![
                ready(SESSION),
                read_tool(),
                read_question(),
                BackendEvent::ToolDone {
                    id: "call_001".into(),
                    output: "Found 3 configuration files...".into(),
                    failed: false,
                },
                ended(false),
            ],
            sent: &[
                "initialize",
                "session/new",
                "session/prompt",
                "reply 5 allow-once",
            ],
        },
        Case {
            name: "cancel answers the pending permission request",
            resume: None,
            lines: include_str!("../../../fixtures/acp/1.32.0/cancel.jsonl"),
            commands: vec![(0, send("Read the config")), (4, BackendCommand::Interrupt)],
            events: vec![ready(SESSION), read_tool(), read_question(), ended(true)],
            sent: &[
                "initialize",
                "session/new",
                "session/prompt",
                "session/cancel",
                "reply 5 cancelled",
            ],
        },
        Case {
            name: "resume when the agent can",
            resume: Some("sess_789xyz"),
            lines: include_str!("../../../fixtures/acp/1.32.0/resume.jsonl"),
            commands: vec![(0, send("Go on"))],
            events: vec![ready("sess_789xyz")],
            sent: &["initialize", "session/resume", "session/prompt"],
        },
        Case {
            name: "load when the agent can, without showing the replay",
            resume: Some("sess_789xyz"),
            lines: include_str!("../../../fixtures/acp/1.32.0/load.jsonl"),
            commands: vec![(0, send("Go on"))],
            events: vec![ready("sess_789xyz")],
            sent: &["initialize", "session/load", "session/prompt"],
        },
        Case {
            name: "a new session when the agent cannot resume",
            resume: Some("sess_789xyz"),
            lines: include_str!("../../../fixtures/acp/1.32.0/new-only.jsonl"),
            commands: vec![(0, send("Go on"))],
            events: vec![
                BackendEvent::Notice(
                    "Agent cannot resume its session: a new one started, without the earlier messages."
                        .into(),
                ),
                ready(SESSION),
            ],
            sent: &["initialize", "session/new", "session/prompt"],
        },
        Case {
            name: "garbage, unknown messages and unsupported requests",
            resume: None,
            lines: concat!(
                "not json\n",
                "\n",
                "{\"jsonrpc\":\"2.0\",\"id\":0,\n",
                "42\n",
                "{\"jsonrpc\":\"2.0\",\"method\":\"_vendor/notice\",\"params\":{}}\n",
                "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"s\",\"update\":{\"sessionUpdate\":\"future_update\",\"x\":1}}}\n",
                "{\"jsonrpc\":\"2.0\",\"id\":77,\"result\":{}}\n",
                "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"fs/read_text_file\",\"params\":{\"path\":\"/etc/hosts\"}}\n",
            ),
            commands: Vec::new(),
            events: Vec::new(),
            sent: &["initialize", "reply 9 Method not found"],
        },
        Case {
            name: "another protocol version fails",
            resume: None,
            lines: "{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":2}}\n",
            commands: vec![(0, send("hi"))],
            events: vec![BackendEvent::Failed(
                "Agent speaks ACP version 2; Leon speaks version 1".into(),
            )],
            sent: &["initialize"],
        },
        Case {
            name: "a refused session fails",
            resume: None,
            lines: concat!(
                "{\"jsonrpc\":\"2.0\",\"id\":0,\"result\":{\"protocolVersion\":1}}\n",
                "{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32000,\"message\":\"Authentication required\"}}\n",
            ),
            commands: vec![(0, send("hi"))],
            events: vec![BackendEvent::Failed("Authentication required".into())],
            sent: &["initialize", "session/new"],
        },
        Case {
            name: "a refused prompt ends the turn",
            resume: None,
            lines: concat!(
                include_str!("../../../fixtures/acp/1.32.0/new-only.jsonl"),
                "{\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"code\":-32603,\"message\":\"Internal error\"}}\n",
            ),
            commands: vec![(0, send("hi"))],
            events: vec![
                ready(SESSION),
                BackendEvent::Failed("Internal error".into()),
                ended(false),
            ],
            sent: &["initialize", "session/new", "session/prompt"],
        },
    ]
}

#[test]
fn replayed_exchanges() {
    for case in cases() {
        let (events, sent) = replay(&spec(case.resume), case.lines, case.commands);
        assert_eq!(events, case.events, "{}", case.name);
        let sent: Vec<String> = sent.iter().map(describe).collect();
        assert_eq!(sent, case.sent, "{}", case.name);
    }
}
