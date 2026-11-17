//! Contract tests against the real CLIs. Opt-in: they need a logged-in CLI
//! and spend a few tokens. Run with `cargo test -p leon-backends -- --ignored`.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::time::Duration;

use leon_backends::Clis;
use leon_core::ports::{BackendCommand, BackendEvent, Backends, SessionSpec};
use leon_domain::{BackendKind, SessionSettings};

async fn pong(backend: BackendKind, model: Option<&str>) -> Vec<BackendEvent> {
    let dir = tempfile::tempdir().unwrap();
    let spec = SessionSpec {
        settings: SessionSettings {
            backend,
            agent: None,
            model: model.map(str::to_owned),
            effort: Some("low".into()),
            permission: Clis::default().options(backend).permissions[0].clone(),
        },
        agent: None,
        working_dir: dir.path().to_path_buf(),
        resume: None,
        mcp_servers: Vec::new(),
        sources: Vec::new(),
    };
    let mut channel = Clis::default().start(spec).unwrap();
    channel
        .commands
        .send(BackendCommand::Send(
            "Reply with the single word: pong".into(),
        ))
        .unwrap();
    let mut events = Vec::new();
    while let Ok(Some(event)) =
        tokio::time::timeout(Duration::from_secs(120), channel.events.recv()).await
    {
        let done = matches!(event, BackendEvent::TurnEnded { .. });
        events.push(event);
        if done {
            break;
        }
    }
    events
}

fn assert_pong(events: &[BackendEvent]) {
    assert!(
        matches!(events.first(), Some(BackendEvent::Ready { .. })),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, BackendEvent::Text(t) if t.to_lowercase().contains("pong"))),
        "{events:?}"
    );
    assert!(
        matches!(
            events.last(),
            Some(BackendEvent::TurnEnded { stopped: false, .. })
        ),
        "{events:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a logged-in Codex CLI"]
async fn codex_answers() {
    assert_pong(&pong(BackendKind::Codex, None).await);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a logged-in Claude Code CLI"]
async fn claude_answers() {
    assert_pong(&pong(BackendKind::ClaudeCode, Some("haiku")).await);
}
