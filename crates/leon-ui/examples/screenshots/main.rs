//! Renders Leon's main screens offscreen with sample data and saves them as
//! PNG files. `cargo run -p leon-ui --example screenshots --features preview
//! -- <folder>`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_core::ports::{AgentDefinition, BackendOptions, SlashCommand};
use leon_core::{ConversationView, Detail, Question, SessionView, Snapshot, TranscriptView};
use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, Conversation, ConversationId, Entry, PlanStep,
    Project, ProjectId, SessionSettings, Settings, Status, StepStatus, TimedEntry, Usage,
};
use leon_ui::preview::{Shot, capture};

mod repo;

fn models(items: &[(&str, &str)]) -> Vec<leon_core::ports::ModelOption> {
    items
        .iter()
        .map(|(id, label)| leon_core::ports::ModelOption {
            aliases: Vec::new(),
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
            // planner (1) → coder (2) → reviewer (4), who reports back to the
            // planner: a loop.
            started_by: match id {
                2 => Some(AgentSessionId(1)),
                4 => Some(AgentSessionId(2)),
                _ => None,
            },
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
        plan: if id == 1 {
            vec![
                PlanStep::new("Find where retries stop", StepStatus::Done),
                PlanStep::new("Raise the window to 15 minutes", StepStatus::Done),
                PlanStep::new("Cap each delay at 2 minutes", StepStatus::InProgress),
                PlanStep::new("Test the delay sequence", StepStatus::Pending),
            ]
        } else {
            Vec::new()
        },
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
        updated_at: hours_ago(id * 5),
        working_dir: None,
        // The first conversation was started for issue #401.
        issue: (id == 1).then(|| leon_domain::GhRef::issue("acme/payments-api", 401)),
    }
}

/// A time `hours` before now, Unix milliseconds.
pub fn hours_ago(hours: i64) -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    now - hours * 3_600_000
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
            from: AgentSessionId(4),
            text: "Before you merge: add a test for attempt 0.".into(),
            answer: false,
        }),
        timed(Entry::FromAgent {
            from: AgentSessionId(2),
            text: "Done: the window is 15 minutes and each delay is capped at 2 minutes.".into(),
            answer: true,
        }),
        timed(Entry::Notice { text: "From your next message: model sonnet.".into() }),
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
        AgentSessionId(4),
        Arc::new(TranscriptView {
            entries: Arc::new(vec![
                timed(Entry::FromAgent {
                    from: AgentSessionId(2),
                    text: "Review the retry change.".into(),
                    answer: false,
                }),
                timed(Entry::Agent {
                    text: "One finding: no test for attempt 0.".into(),
                }),
                timed(Entry::TurnEnd { usage: None }),
            ]),
            partial: None,
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
            sessions: vec![AgentSessionId(1), AgentSessionId(2), AgentSessionId(4)],
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
        session(4, 1, "reviewer", BackendKind::ClaudeCode, Status::Idle),
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
                ..Default::default()
            },
            AgentDefinition {
                name: "coder".into(),
                description: "Implements an approved plan in the worktree it is given: \
                              writes the code and its tests, runs them, and reports what \
                              changed. Use it once the plan is approved, never to explore \
                              or to decide what to build."
                    .into(),
                instructions: String::new(),
                ..Default::default()
            },
        ],
    )]);
    let command = |name: &str, description: &str, hint: Option<&str>| SlashCommand {
        name: name.into(),
        description: description.into(),
        argument_hint: hint.map(Into::into),
    };
    let commands = HashMap::from([(
        ProjectId(1),
        vec![
            command("deploy", "Deploy a branch", Some("[environment] [version]")),
            command("deps-audit", "Check dependencies for advisories", None),
            command("review", "Review the current branch", Some("[pr-number]")),
        ],
    )]);
    Snapshot {
        loaded: true,
        commands: Arc::new(commands),
        settings: Settings {
            diff_fold: vec!["glob:**/*.lock".into()],
            sources: vec!["~/agent-kit".into()],
            cli_order: vec![leon_domain::BackendKind::Codex],
            ..Settings::default()
        },
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
        folders: Arc::new(vec![
            leon_core::ports::AgentFolder {
                label: "~/.agents".into(),
                added: false,
                agents: 3,
                skills: 5,
            },
            leon_core::ports::AgentFolder {
                label: "~/.claude".into(),
                added: false,
                agents: 5,
                skills: 1,
            },
            leon_core::ports::AgentFolder {
                label: "/home/user/agent-kit".into(),
                added: true,
                agents: 1,
                skills: 2,
            },
        ]),
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
            "slash-list".to_owned(),
            Shot::Typed(ConversationId(1), "/dep"),
        ),
        (
            "slash-arguments".to_owned(),
            Shot::Typed(ConversationId(1), "/deploy staging"),
        ),
        (
            "picker".to_owned(),
            Shot::PickerOpen(ConversationId(1), "session-model"),
        ),
        ("flow".to_owned(), Shot::Flow(ConversationId(1))),
        ("new-conversation".to_owned(), Shot::NewConversation),
        (
            "new-conversation-agent".to_owned(),
            Shot::NewConversationWith("coder"),
        ),
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
        ("my-issues".to_owned(), Shot::MyIssues),
        ("settings".to_owned(), Shot::Settings),
        ("doctor".to_owned(), Shot::Doctor),
        ("row-menu".to_owned(), Shot::RowMenu(ConversationId(1))),
        ("tools".to_owned(), Shot::Tools),
        // Last: these leave the details drawer open.
        (
            "issue-detail".to_owned(),
            Shot::Detail(Box::new(Detail::Issue(Box::new(
                repo::my_issues()[0].clone(),
            )))),
        ),
        (
            "pr-detail".to_owned(),
            Shot::Detail(Box::new(Detail::PullRequest(Box::new(
                repo::prs()[&ConversationId(1)].clone(),
            )))),
        ),
        (
            "drawer".to_owned(),
            Shot::Drawer(ConversationId(1), "src/webhooks/retry.rs", false),
        ),
        (
            "drawer-split".to_owned(),
            Shot::Drawer(ConversationId(1), "src/webhooks/retry.rs", true),
        ),
    ];
    gpui_platform::application()
        .with_assets(leon_ui::Assets)
        .run(move |cx| capture(&core, shots, out, cx));
}
