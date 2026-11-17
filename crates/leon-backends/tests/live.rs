//! Contract tests against the real CLIs. Opt-in: they need a logged-in CLI
//! and spend a few tokens. Run with `cargo test -p leon-backends -- --ignored`.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::time::Duration;

use leon_backends::Clis;
use leon_core::ports::{BackendChannel, BackendCommand, BackendEvent, Backends, SessionSpec};
use leon_domain::{BackendKind, SessionSettings};

fn spec(backend: BackendKind, dir: &std::path::Path, resume: Option<String>) -> SessionSpec {
    SessionSpec {
        settings: SessionSettings {
            backend,
            agent: None,
            model: None,
            effort: Some("low".into()),
            permission: Clis::default().options(backend).permissions[0].clone(),
        },
        agent: None,
        working_dir: dir.to_path_buf(),
        resume,
        mcp_servers: Vec::new(),
        sources: Vec::new(),
    }
}

/// Sends `text` and collects events until the turn ends; `stop_after` sends
/// a Stop once that many text deltas arrived.
async fn turn(
    channel: &mut BackendChannel,
    text: &str,
    stop_after: Option<usize>,
) -> Vec<BackendEvent> {
    channel
        .commands
        .send(BackendCommand::Send(text.into()))
        .unwrap();
    let mut events = Vec::new();
    let mut deltas = 0;
    while let Ok(Some(event)) =
        tokio::time::timeout(Duration::from_secs(120), channel.events.recv()).await
    {
        let done = matches!(event, BackendEvent::TurnEnded { .. });
        if matches!(event, BackendEvent::TextDelta(_)) {
            deltas += 1;
            if stop_after == Some(deltas) {
                channel.commands.send(BackendCommand::Interrupt).unwrap();
            }
        }
        events.push(event);
        if done {
            break;
        }
    }
    events
}

async fn pong(backend: BackendKind, model: Option<&str>) -> Vec<BackendEvent> {
    let dir = tempfile::tempdir().unwrap();
    let mut s = spec(backend, dir.path(), None);
    s.settings.model = model.map(str::to_owned);
    let mut channel = Clis::default().start(s).unwrap();
    turn(&mut channel, "Reply with the single word: pong", None).await
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a logged-in Copilot CLI"]
async fn copilot_answers() {
    assert_pong(&pong(BackendKind::CopilotCli, Some("claude-haiku-4.5")).await);
}

#[test]
#[ignore = "runs the real Codex CLI with the user's login"]
fn codex_writes_a_title() {
    let title = Clis::default()
        .title(
            BackendKind::Codex,
            "webhook retries give up after 30 seconds, make them retry for 15 minutes",
        )
        .unwrap();
    eprintln!("title: {title}");
    assert!(
        (2..=8).contains(&title.split_whitespace().count()),
        "{title}"
    );
}

#[test]
#[ignore = "runs the real Claude Code CLI with the user's login"]
fn claude_writes_a_title() {
    let title = Clis::default()
        .title(
            BackendKind::ClaudeCode,
            "webhook retries give up after 30 seconds, make them retry for 15 minutes",
        )
        .unwrap();
    eprintln!("title: {title}");
    assert!(
        (2..=8).contains(&title.split_whitespace().count()),
        "{title}"
    );
}

/// Stop ends Claude Code's process (I2); the next message resumes the same
/// CLI session, which still knows the conversation.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code CLI with the user's login"]
async fn claude_stops_then_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let clis = Clis::default();
    let mut channel = clis
        .start(spec(BackendKind::ClaudeCode, dir.path(), None))
        .unwrap();
    let first = turn(
        &mut channel,
        "Remember the word otter. Then write the numbers from 1 to 300, one per line.",
        Some(3),
    )
    .await;
    let Some(BackendEvent::Ready { cli_session }) = first.first().cloned() else {
        panic!("no session id: {first:?}");
    };
    assert!(
        matches!(
            first.last(),
            Some(BackendEvent::TurnEnded { stopped: true, .. })
        ),
        "{first:?}"
    );
    drop(channel);

    let mut resumed = clis
        .start(spec(BackendKind::ClaudeCode, dir.path(), Some(cli_session)))
        .unwrap();
    let second = turn(
        &mut resumed,
        "Which word did I ask you to remember? Reply with that word only.",
        None,
    )
    .await;
    eprintln!("{second:?}");
    assert!(
        second
            .iter()
            .any(|e| matches!(e, BackendEvent::Text(t) if t.to_lowercase().contains("otter"))),
        "{second:?}"
    );
}

#[test]
#[ignore = "runs the real CLIs' list commands with the user's config"]
fn connectors_are_listed() {
    for cli in Clis::default().connectors() {
        let mut counts = std::collections::BTreeMap::new();
        for c in &cli.connectors {
            *counts
                .entry(format!("{:?} {:?}", c.kind, c.state))
                .or_insert(0) += 1;
        }
        eprintln!("{:?}: {counts:?}, problem: {:?}", cli.backend, cli.problem);
        assert_eq!(cli.problem, None, "{:?}", cli.backend);
        assert!(!cli.connectors.is_empty(), "{:?}", cli.backend);
    }
}
