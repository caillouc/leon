//! Opt-in contract test (AGENTS.md): a real CLI starts `leon mcp` from its
//! conversation config and calls one of Leon's tools through it (P10). Uses the
//! user's own CLI login; not run in CI.
//! `cargo test -p leon --test live_tools -- --ignored --nocapture`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "test"
)]

use std::path::PathBuf;
use std::time::Duration;

use leon_app_tools::LocalTools;
use leon_backends::Clis;
use leon_core::ports::{
    AppToolCall, AppTools, BackendCommand, BackendEvent, Backends, SessionSpec,
};
use leon_domain::{BackendKind, ConversationId, ConversationSettings};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Codex CLI with the user's login"]
async fn codex_calls_a_leon_tool() {
    calls_a_leon_tool(BackendKind::Codex).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code CLI with the user's login"]
async fn claude_calls_a_leon_tool() {
    calls_a_leon_tool(BackendKind::ClaudeCode).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Copilot CLI with the user's login"]
async fn copilot_calls_a_leon_tool() {
    calls_a_leon_tool(BackendKind::CopilotCli).await;
}

async fn calls_a_leon_tool(backend: BackendKind) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("tools.sock");
    let (tools_tx, mut requests) = tokio::sync::mpsc::unbounded_channel();
    leon_app_tools::serve(&tokio::runtime::Handle::current(), &socket, tools_tx).unwrap();
    let tools = LocalTools::new(PathBuf::from(env!("CARGO_BIN_EXE_leon")), socket);
    let clis = Clis::default();
    let spec = SessionSpec {
        conversation: leon_domain::ConversationId(1),
        settings: ConversationSettings {
            backend,
            agent: None,
            model: None,
            effort: Some("low".into()),
            permission: clis.options(backend).permissions[0].clone(),
        },
        agent: None,
        working_dir: dir.path().to_path_buf(),
        resume: None,
        mcp_servers: vec![tools.server(ConversationId(7))],
        sources: Vec::new(),
        skills: Vec::new(),
        instructions: None,
        app_instructions: None,
    };
    let mut channel = clis.start(spec).unwrap();
    channel
        .commands
        .send(BackendCommand::Send(
            "Call the tool rename_flow of the leon MCP server with the title \"pong\", then reply with the single word: done".into(),
        ))
        .unwrap();
    let mut called = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        tokio::select! {
            Some(request) = requests.recv() => {
                called = Some((request.conversation, request.call.clone()));
                request.reply.send(Ok("Renamed the conversation.".into())).unwrap();
            }
            event = channel.events.recv() => match event {
                Some(BackendEvent::Question { id, title, options, .. }) => {
                    eprintln!("question: {title} {options:?}");
                    let answer = options.first().cloned().unwrap_or_default();
                    channel.commands.send(BackendCommand::Answer { id, answer }).unwrap();
                }
                Some(BackendEvent::TurnEnded { .. }) | None => break,
                Some(other) => eprintln!("{other:?}"),
            },
            () = tokio::time::sleep_until(deadline) => panic!("no answer in 120 s"),
        }
    }
    assert_eq!(
        called,
        Some((
            ConversationId(7),
            AppToolCall::RenameFlow {
                title: "pong".into()
            }
        ))
    );
}
