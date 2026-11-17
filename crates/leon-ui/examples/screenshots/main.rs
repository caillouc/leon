//! Renders Leon's main screens offscreen with sample data and saves them as
//! PNG files. `cargo run -p leon-ui --example screenshots --features preview
//! -- <folder>`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_core::ports::{AgentDefinition, BackendOptions};
use leon_core::{ConversationView, Question, SessionView, Snapshot, TranscriptView};
use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, Conversation, ConversationId, Entry, Project,
    ProjectId, SessionSettings, Settings, Status, TimedEntry, Usage,
};
use leon_ui::preview::{Shot, capture};

mod repo;

fn models(items: &[(&str, &str)]) -> Vec<leon_core::ports::ModelOption> {
    items
        .iter()
        .map(|(id, label)| leon_core::ports::ModelOption {
            id: (*id).into(),
            label: (*label).into(),
        })
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn session(
    id: i64,
    conversation: i64,
    agent: &str,
    backend: BackendKind,
    status: Status,
) -> SessionView {
    SessionView {
        session: AgentSession {
            id: AgentSessionId(id),
            conversation: ConversationId(conversation),
            started_by: (id != 1 && id != 3).then_some(AgentSessionId(1)),
            settings: SessionSettings {
                backend,
                agent: Some(agent.to_owned()),
                model: Some("claude-opus-5-5".to_owned()),
                effort: Some("high".to_owned()),
                permission: "auto".to_owned(),
            },
            working_dir: PathBuf::from("/code/payments-api/.worktrees/fix-webhook-retry"),
            cli_session: Some("s".to_owned()),
        },
        status,
        subagents: if id == 1 { (2, 1) } else { (0, 0) },
        question: (id == 3).then(|| Question {
            id: "q".into(),
            title: "Run this command?".into(),
            detail: "cargo test --workspace".into(),
            options: strings(&["Allow once", "Deny"]),
        }),
        suggestions: if id == 3 {
            strings(&["Run the tests", "Open a draft PR"])
        } else {
            Vec::new()
        },
    }
}

fn conversation(id: i64, project: i64, title: &str, unread: bool, pinned: bool) -> Conversation {
    Conversation {
        id: ConversationId(id),
        project: ProjectId(project),
        title: title.into(),
        pinned,
        archived: false,
        unread,
        updated_at: 0,
        working_dir: None,
    }
}

fn timed(entry: Entry) -> TimedEntry {
    TimedEntry { at: 0, entry }
}

/// Transcripts of the sample sessions.
fn transcripts() -> HashMap<AgentSessionId, Arc<TranscriptView>> {
    let transcript = vec![
        timed(Entry::User { text: "/ticket 398 webhook retries give up after 30 seconds".into() }),
        timed(Entry::Tool { id: "1".into(), name: "Read".into(), detail: "src/webhooks/retry.rs".into(), output: Some("…".into()), failed: false }),
        timed(Entry::Tool { id: "2".into(), name: "Ran".into(), detail: "cargo test webhook".into(), output: Some("test result: ok. 6 passed".into()), failed: false }),
        timed(Entry::Agent {
            text: "**Recap.** Deliveries stop retrying after 30 s (`MAX_ELAPSED` in `src/webhooks/retry.rs:13`).\n\n**Plan**\n1. Raise the retry window to 15 minutes.\n2. Cap each delay at 2 minutes.\n3. Test the delay sequence.\n\n```rust\nconst MAX_DELAY: Duration = Duration::from_secs(120);\n```".into(),
        }),
        timed(Entry::TurnEnd { usage: Some(Usage { input_tokens: 41_200, output_tokens: 1_830, cost_usd: Some(0.42) }) }),
        timed(Entry::User { text: "Go ahead".into() }),
        timed(Entry::FromAgent {
            from: AgentSessionId(2),
            text: "Done: the window is 15 minutes and each delay is capped at 2 minutes.".into(),
            answer: true,
        }),
        timed(Entry::Notice { text: "Changed model sonnet from your next message.".into() }),
        timed(Entry::Tool { id: "3".into(), name: "Edited".into(), detail: "src/webhooks/retry.rs".into(), output: None, failed: false }),
    ];
    let mut transcripts = HashMap::new();
    transcripts.insert(
        AgentSessionId(1),
        Arc::new(TranscriptView {
            entries: Arc::new(transcript.clone()),
            partial: Some("Editing the retry loop: the delay now".into()),
        }),
    );
    transcripts.insert(
        AgentSessionId(2),
        Arc::new(TranscriptView {
            entries: Arc::new(vec![
                timed(Entry::FromAgent {
                    from: AgentSessionId(1),
                    text: "Implement steps 1 and 2 of the plan in `src/webhooks/retry.rs`.".into(),
                    answer: false,
                }),
                timed(Entry::Tool {
                    id: "c1".into(),
                    name: "Edited".into(),
                    detail: "src/webhooks/retry.rs".into(),
                    output: Some(String::new()),
                    failed: false,
                }),
                timed(Entry::Agent {
                    text: "Done: the window is 15 minutes and each delay is capped at 2 minutes."
                        .into(),
                }),
                timed(Entry::TurnEnd { usage: None }),
            ]),
            partial: None,
        }),
    );
    transcripts.insert(
        AgentSessionId(3),
        Arc::new(TranscriptView {
            entries: Arc::new(transcript[..4].to_vec()),
            partial: None,
        }),
    );
    transcripts
}

/// What the sample CLIs offer.
fn options() -> HashMap<BackendKind, BackendOptions> {
    HashMap::from([
        (
            BackendKind::ClaudeCode,
            BackendOptions {
                models: models(&[
                    ("claude-opus-5-5", "Opus 5.5"),
                    ("claude-sonnet-5-5", "Sonnet 5.5"),
                    ("claude-haiku-4-5", "Haiku 4.5"),
                ]),
                efforts: strings(&["low", "medium", "high", "xhigh", "max"]),
                default_effort: Some("high".into()),
                permissions: strings(&[
                    "auto",
                    "bypassPermissions",
                    "acceptEdits",
                    "plan",
                    "manual",
                ]),
            },
        ),
        (
            BackendKind::Codex,
            BackendOptions {
                models: models(&[
                    ("gpt-6-luna", "GPT-6-Luna"),
                    ("gpt-5.6-terra", "GPT-5.6-Terra"),
                ]),
                efforts: strings(&["low", "medium", "high"]),
                default_effort: Some("medium".into()),
                permissions: strings(&["auto", "full access", "on request", "read only"]),
            },
        ),
    ])
}

fn sample() -> Snapshot {
    let conversations = vec![
        ConversationView {
            conversation: conversation(1, 1, "Fix webhook retry timeout", false, false),
            status: Status::Running,
            sessions: vec![AgentSessionId(1), AgentSessionId(2)],
        },
        ConversationView {
            conversation: conversation(2, 2, "Migrate config to TOML", true, false),
            status: Status::NeedsInput,
            sessions: vec![AgentSessionId(3)],
        },
        ConversationView {
            conversation: conversation(3, 1, "Draft release notes v2.3", false, true),
            status: Status::Idle,
            sessions: vec![],
        },
        ConversationView {
            conversation: conversation(4, 2, "Investigate flaky login e2e test", true, false),
            status: Status::Error,
            sessions: vec![],
        },
    ];
    let sessions = [
        session(1, 1, "planner", BackendKind::ClaudeCode, Status::Running),
        session(2, 1, "coder", BackendKind::Codex, Status::Idle),
        session(3, 2, "coder", BackendKind::Codex, Status::NeedsInput),
    ];
    let transcripts = transcripts();
    let options = options();
    let agents = HashMap::from([(
        ProjectId(1),
        vec![
            AgentDefinition {
                name: "planner".into(),
                description: "Understands the issue and writes the plan.".into(),
                instructions: String::new(),
            },
            AgentDefinition {
                name: "coder".into(),
                description: "Implements an approved plan.".into(),
                instructions: String::new(),
            },
        ],
    )]);
    Snapshot {
        loaded: true,
        settings: Settings::default(),
        projects: Arc::new(vec![
            Project {
                id: ProjectId(1),
                name: "payments-api".into(),
                path: "/code/payments-api".into(),
                collapsed: false,
            },
            Project {
                id: ProjectId(2),
                name: "infra-tools".into(),
                path: "/code/infra-tools".into(),
                collapsed: true,
            },
        ]),
        conversations: Arc::new(conversations),
        sessions: Arc::new(sessions.into_iter().map(|s| (s.session.id, s)).collect()),
        transcripts,
        agents: Arc::new(agents),
        options: Arc::new(options),
        git: repo::git(),
        prs: repo::prs(),
        my_issues: repo::my_issues(),
        my_prs: repo::my_prs(),
        fold: repo::fold(),
        doctor: Some(repo::doctor()),
        shared_tools: repo::shared_tools(),
        shared_tools_path: "~/.config/leon/mcp.json".into(),
        tools: Some(repo::tools()),
        notifications: Arc::new(Vec::new()),
        ..Snapshot::default()
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/screenshots".to_owned()),
    );
    let _ = std::fs::create_dir_all(&out);
    let (core, _detached) = leon_core::detached(sample());
    let shots = vec![
        ("empty".to_owned(), Shot::Empty),
        (
            "conversation".to_owned(),
            Shot::Conversation(ConversationId(1)),
        ),
        ("question".to_owned(), Shot::Conversation(ConversationId(2))),
        (
            "picker".to_owned(),
            Shot::PickerOpen(ConversationId(1), "session-model"),
        ),
        ("flow".to_owned(), Shot::Flow(ConversationId(1))),
        ("new-conversation".to_owned(), Shot::NewConversation),
        (
            "right-branch".to_owned(),
            Shot::Right(ConversationId(1), "branch"),
        ),
        ("right-pr".to_owned(), Shot::Right(ConversationId(1), "pr")),
        (
            "right-info".to_owned(),
            Shot::Right(ConversationId(1), "info"),
        ),
        ("review".to_owned(), Shot::Review(ConversationId(1))),
        ("my-prs".to_owned(), Shot::MyPullRequests),
        ("doctor".to_owned(), Shot::Doctor),
        ("row-menu".to_owned(), Shot::RowMenu(ConversationId(1))),
        ("tools".to_owned(), Shot::Tools),
    ];
    gpui_platform::application().run(move |cx| capture(&core, shots, out, cx));
}
