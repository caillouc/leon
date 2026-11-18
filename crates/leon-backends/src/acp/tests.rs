//! Replays the ACP spec's own examples (fixtures/acp/1.32.0).

use std::path::PathBuf;

use leon_core::ports::{AgentDefinition, BackendCommand, BackendEvent, SessionSpec, Source};
use leon_domain::{BackendKind, McpServer, SessionSettings};
use serde_json::{Value, json};

use super::copilot;
use super::session::Session;

mod replays;

const SESSION: &str = "sess_abc123def456";

fn spec(resume: Option<&str>) -> SessionSpec {
    SessionSpec {
        settings: SessionSettings {
            backend: BackendKind::CopilotCli,
            agent: None,
            model: None,
            effort: None,
            permission: "auto".into(),
        },
        agent: None,
        working_dir: PathBuf::from("/home/user/project"),
        resume: resume.map(str::to_owned),
        mcp_servers: Vec::new(),
        sources: Vec::new(),
        skills: Vec::new(),
        instructions: None,
    }
}

fn server(name: &str, command: &str) -> McpServer {
    McpServer {
        name: name.into(),
        command: PathBuf::from(command),
        args: vec!["mcp".into()],
        env: vec![("LEON_SESSION".into(), "7".into())],
        trusted: name == "leon",
    }
}

/// Feeds `lines` to a session, sending each command once `after` lines were
/// read; returns the events and every message Leon sent.
fn replay(
    spec: &SessionSpec,
    lines: &str,
    commands: Vec<(usize, BackendCommand)>,
) -> (Vec<BackendEvent>, Vec<Value>) {
    let (mut session, mut sent) = Session::new(spec, "Agent");
    let mut events = Vec::new();
    let mut commands = commands.into_iter().peekable();
    for (read, line) in std::iter::once("").chain(lines.lines()).enumerate() {
        // As the runner does: lines that are not JSON are skipped.
        if read > 0
            && let Ok(msg) = serde_json::from_str::<serde_json::Value>(line)
        {
            let (new_events, replies) = session.handle(&msg);
            events.extend(new_events);
            sent.extend(replies);
        }
        while let Some((_, command)) = commands.next_if(|(after, _)| *after == read) {
            sent.extend(session.command(command));
        }
    }
    (events, sent)
}

/// A sent message in a few words: its method, or `reply <id> <outcome>`.
fn describe(msg: &Value) -> String {
    if let Some(method) = msg["method"].as_str() {
        return method.to_owned();
    }
    let outcome = &msg["result"]["outcome"];
    let what = outcome["optionId"]
        .as_str()
        .or(outcome["outcome"].as_str())
        .or(msg["error"]["message"].as_str())
        .unwrap_or("?");
    format!("reply {} {what}", msg["id"])
}

fn send(text: &str) -> BackendCommand {
    BackendCommand::Send(text.into())
}

fn ready(id: &str) -> BackendEvent {
    BackendEvent::Ready {
        cli_session: id.into(),
    }
}

fn ended(stopped: bool) -> BackendEvent {
    BackendEvent::TurnEnded {
        stopped,
        usage: None,
    }
}

#[test]
fn selectors_match_by_id_or_name() {
    let mut s = spec(None);
    s.settings.model = Some("Model 2".into());
    let lines = include_str!("../../fixtures/acp/1.32.0/config.jsonl");
    let (events, sent) = replay(&s, lines, vec![(0, send("hi"))]);
    assert_eq!(events, [ready(SESSION)]);
    // The prompt waits for the answer.
    let methods: Vec<String> = sent.iter().map(describe).collect();
    assert_eq!(
        methods,
        [
            "initialize",
            "session/new",
            "session/set_config_option",
            "session/prompt"
        ]
    );
    assert_eq!(
        sent[2]["params"],
        json!({ "sessionId": SESSION, "configId": "model", "value": "model-2" })
    );
}

#[test]
fn mcp_servers_go_in_copilots_extra_config_not_the_session_request() {
    let mut s = spec(None);
    s.mcp_servers = vec![server("leon", "/bin/leon")];
    s.sources = vec![Source {
        dir: PathBuf::from("/plugins/mine"),
        mcp_servers: vec![server("leon", "/bin/other"), server("docs", "/bin/docs")],
    }];
    // Copilot CLI drops stdio servers sent over ACP (1.0.113 log: "Rejecting
    // non-http/sse MCP server").
    let lines = include_str!("../../fixtures/acp/1.32.0/new-only.jsonl");
    let (_, sent) = replay(&s, lines, Vec::new());
    assert_eq!(
        sent[1]["params"],
        json!({ "cwd": "/home/user/project", "mcpServers": [] })
    );
    let env = json!({ "LEON_SESSION": "7" });
    let extra = json!({ "mcpServers": {
        "leon": { "command": "/bin/leon", "args": ["mcp"], "env": env },
        "docs": { "command": "/bin/docs", "args": ["mcp"], "env": env },
    } });
    assert!(
        copilot(&s, None)
            .args
            .contains(&format!("--additional-mcp-config={extra}")),
        "{:?}",
        copilot(&s, None).args
    );
}

#[test]
fn agent_instructions_go_in_the_system_prompt_or_the_first_prompt() {
    let mut s = spec(None);
    s.agent = Some(AgentDefinition {
        name: "reviewer".into(),
        description: String::new(),
        instructions: "Focus on security issues.".into(),
        ..Default::default()
    });
    let with_system_prompt = include_str!("../../fixtures/acp/1.32.0/system-prompt.jsonl");
    let commands = || vec![(0, send("first")), (2, send("second"))];
    let (_, sent) = replay(&s, with_system_prompt, commands());
    assert_eq!(
        sent[1]["params"]["systemPrompt"],
        json!({ "mode": "append", "content": [{ "type": "text", "text": "Focus on security issues." }] })
    );
    assert_eq!(sent[2]["params"]["prompt"][0]["text"], "first");

    let without = include_str!("../../fixtures/acp/1.32.0/new-only.jsonl");
    let (_, sent) = replay(&s, without, commands());
    assert!(sent[1]["params"].get("systemPrompt").is_none());
    assert_eq!(
        sent[2]["params"]["prompt"][0]["text"],
        "Focus on security issues.\n\nfirst"
    );
    assert_eq!(sent[3]["params"]["prompt"][0]["text"], "second");
}

#[test]
fn messages_split_on_a_new_message_id() {
    let (mut session, _) = Session::new(&spec(None), "Agent");
    let chunk = |id: &str, text: &str| {
        json!({ "method": "session/update", "params": { "update": {
            "sessionUpdate": "agent_message_chunk", "messageId": id,
            "content": { "type": "text", "text": text } } } })
    };
    let mut events = Vec::new();
    for msg in [chunk("m1", "a"), chunk("m1", "b"), chunk("m2", "c")] {
        events.extend(session.handle(&msg).0);
    }
    assert_eq!(
        events,
        [
            BackendEvent::TextDelta("a".into()),
            BackendEvent::TextDelta("b".into()),
            BackendEvent::Text("ab".into()),
            BackendEvent::TextDelta("c".into()),
        ]
    );
}

#[test]
fn allow_once_comes_first_and_free_text_rejects() {
    let (mut session, _) = Session::new(&spec(None), "Agent");
    let request = json!({ "jsonrpc": "2.0", "id": "p1", "method": "session/request_permission", "params": {
        "toolCall": { "toolCallId": "t1", "title": "rm -rf build", "kind": "execute" },
        "options": [
            { "optionId": "always", "name": "Always allow", "kind": "allow_always" },
            { "optionId": "once", "name": "Allow", "kind": "allow_once" },
            { "optionId": "no", "name": "Deny", "kind": "reject_once" },
        ] } });
    let (events, _) = session.handle(&request);
    assert_eq!(
        events,
        [BackendEvent::Question {
            id: "acp-p1".into(),
            title: "Run this command?".into(),
            detail: "rm -rf build".into(),
            options: vec!["Allow".into(), "Always allow".into(), "Deny".into()],
        }]
    );
    let replies = session.command(BackendCommand::Answer {
        id: "acp-p1".into(),
        answer: "not now".into(),
    });
    assert_eq!(
        replies,
        [
            json!({ "jsonrpc": "2.0", "id": "p1", "result": { "outcome": { "outcome": "selected", "optionId": "no" } } })
        ]
    );
}

#[test]
fn copilot_flags() {
    let mut s = spec(None);
    s.settings.model = Some("gpt-6-luna".into());
    s.settings.effort = Some("low".into());
    s.mcp_servers = vec![server("leon", "/bin/leon")];
    let extra = json!({ "mcpServers": { "leon": {
        "command": "/bin/leon", "args": ["mcp"], "env": { "LEON_SESSION": "7" }
    } } });
    assert_eq!(
        copilot(&s, None).args,
        [
            "--acp".to_owned(),
            "--stdio".to_owned(),
            "--model=gpt-6-luna".to_owned(),
            "--effort=low".to_owned(),
            "--allow-all-tools".to_owned(),
            "--allow-tool=leon".to_owned(),
            format!("--additional-mcp-config={extra}"),
        ]
    );
    // Models whose session refuses an effort get none (1.0.113, 1.0.114).
    let mut haiku = s.clone();
    haiku.settings.model = Some("claude-haiku-4.5".into());
    assert!(
        !copilot(&haiku, None)
            .args
            .iter()
            .any(|a| a.starts_with("--effort")),
        "{:?}",
        copilot(&haiku, None).args
    );
    // An agent that may read and run commands only (EXT-7).
    s.agent = Some(leon_core::ports::AgentDefinition {
        tools: Some(vec![
            leon_domain::Capability::Read,
            leon_domain::Capability::Execute,
        ]),
        ..Default::default()
    });
    let denied: Vec<String> = copilot(&s, None)
        .args
        .into_iter()
        .filter(|a| a.starts_with("--deny-tool="))
        .collect();
    assert_eq!(denied, ["--deny-tool=write", "--deny-tool=url"]);
    assert_eq!(copilot(&s, None).env, Vec::<(String, String)>::new());
    s.instructions = Some(leon_core::ports::SharedInstructions {
        file: PathBuf::from("/home/u/.agents/AGENTS.md"),
        text: "Be brief.".into(),
    });
    assert_eq!(
        copilot(&s, None).env,
        [(
            "COPILOT_CUSTOM_INSTRUCTIONS_DIRS".to_owned(),
            "/home/u/.agents".to_owned()
        )]
    );
    let skills = std::path::Path::new("/data/skills/copilot/ab");
    assert_eq!(
        copilot(&s, Some(skills)).args.last().map(String::as_str),
        Some("--plugin-dir=/data/skills/copilot/ab")
    );
}
