//! Renders Leon's main screens offscreen with sample data and saves them as
//! PNG files. `cargo run -p leon-ui --example screenshots --features preview
//! -- <folder>`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_core::ports::{AgentDefinition, BackendOptions, SlashCommand};
use leon_core::{ConversationView, Detail, FlowView, Question, Snapshot, TranscriptView};
use leon_domain::{
    BackendKind, Conversation, ConversationId, ConversationSettings, Entry, Flow, FlowId, PlanStep,
    Project, ProjectId, Settings, Status, StepStatus, TimedEntry, Usage,
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

/// To-dos of the first two flows, one added by an agent, one done, one
/// with a long when and a what of several lines (TODO-1).
fn todos() -> Vec<leon_domain::Todo> {
    let todo = |id, flow, (when, what): (&str, &str), done, by: Option<i64>| leon_domain::Todo {
        id: leon_domain::TodoId(id),
        flow: FlowId(flow),
        when: when.into(),
        what: what.into(),
        done,
        by: by.map(ConversationId),
        added_at: 0,
    };
    vec![
        todo(
            1,
            1,
            ("Once the deploy is done", "Run the backfill"),
            false,
            None,
        ),
        todo(
            2,
            1,
            (
                "After the release sign-off, outside the nightly test runs and the morning deploy window",
                "Merge #412, then:\n- close the release issue\n- tell the team the fix is live",
            ),
            false,
            Some(1),
        ),
        todo(3, 1, ("", "Tell support about the new delay"), true, None),
        todo(4, 2, ("Next Monday", "Rotate the token"), false, None),
        todo(
            5,
            2,
            (
                "Once the deploy is green",
                "Run:\n```sh\nleon-cli migrate --env prod\n```\nthen tell `#ops`.",
            ),
            false,
            Some(5),
        ),
    ]
}

/// A mouse pressed at `x`, `y` then moved down `step` a frame, and a little
/// to the right, ten times.
fn down_and_right(x: f32, y: f32, step: f32) -> Vec<(f32, f32)> {
    let mut path = vec![(x, y), (x + 1., y + 2.)];
    path.extend((1..=10_u8).map(|i| {
        let i = f32::from(i);
        (x + 1. + 3. * i, y + 2. + step * i)
    }));
    path
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn conversation(
    id: i64,
    flow: i64,
    agent: &str,
    backend: BackendKind,
    status: Status,
) -> ConversationView {
    ConversationView {
        conversation: Conversation {
            id: ConversationId(id),
            flow: FlowId(flow),
            // The planner (1) starts two coders (2, 5) and two reviewers
            // (4, 6); the user also asks the second reviewer directly.
            started_by: match id {
                2 | 4 | 5 | 6 => Some(ConversationId(1)),
                _ => None,
            },
            settings: ConversationSettings {
                backend,
                agent: Some(agent.to_owned()),
                model: Some("claude-opus-5-5".to_owned()),
                effort: Some("high".to_owned()),
                permission: "auto".to_owned(),
            },
            working_dir: PathBuf::from("/code/payments-api/.worktrees/fix-webhook-retry"),
            cli_session: Some("s".to_owned()),
            // The reviewer's answer to the user is not read yet.
            unread: id == 4,
            folder: None,
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
            title: "Access paths outside trusted directories".into(),
            detail: "$ cd /code/api && git worktree add -q ../api-fix -b fix origin/main && cd \
                     ../api-fix && make sync >/tmp/sync.log 2>&1; git log --oneline -1\n\
                     /code/api-fix/sync\n/code/api-fix/log"
                .into(),
            options: strings(&["Allow once", "Always allow", "Deny"]),
        }),
        suggestions: if id == 3 {
            strings(&["Run the tests", "Open a draft PR"])
        } else {
            Vec::new()
        },
        // Numbered as the core names them: two coders, two reviewers in
        // the first flow.
        name: match id {
            2 => "coder 1".into(),
            4 => "reviewer 1".into(),
            5 => "coder 2".into(),
            6 => "reviewer 2".into(),
            _ => agent.into(),
        },
        // The second coder and reviewer work on the web dashboard (CTX-2).
        folder: match id {
            1 | 2 | 4 => Some(repo::API.into()),
            5 | 6 => Some(repo::WEB.into()),
            _ => None,
        },
        // The planner waits for the second reviewer's report.
        waiting_for: if id == 1 {
            vec![ConversationId(6)]
        } else {
            Vec::new()
        },
        queued: None,
    }
}

fn flow(id: i64, project: i64, title: &str, unread: bool, pinned: bool) -> Flow {
    Flow {
        id: FlowId(id),
        project: ProjectId(project),
        title: title.into(),
        pinned,
        archived: None,
        unread,
        updated_at: hours_ago(id * 5),
        // The first flow works in two repositories (CTX-2).
        folders: if id == 1 {
            vec![repo::API.into(), repo::WEB.into()]
        } else {
            Vec::new()
        },
        // The first flow was started for issue #401; a first pull request
        // of it is merged.
        links: if id == 1 {
            vec![
                leon_domain::GhRef::issue("acme/payments-api", 401),
                leon_domain::GhRef::pull_request("acme/payments-api", 398),
            ]
        } else {
            Vec::new()
        },
        todo_position: id,
    }
}

/// A time `hours` before now, Unix milliseconds.
pub fn hours_ago(hours: i64) -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    now - hours * 3_600_000
}

/// An entry from two minutes ago: a command still running shows "2m".
fn timed(entry: Entry) -> TimedEntry {
    TimedEntry {
        at: hours_ago(0) - 2 * 60_000,
        entry,
    }
}

/// Transcripts of the sample conversations.
fn transcripts() -> HashMap<ConversationId, Arc<TranscriptView>> {
    let transcript = vec![
        timed(Entry::User { text: "/ticket 398 webhook retries give up after 30 seconds".into() }),
        timed(Entry::Tool { id: "1".into(), name: "Read".into(), detail: "src/webhooks/retry.rs".into(), output: Some("…".into()), failed: false }),
        timed(Entry::Tool { id: "2".into(), name: "Ran".into(), detail: "cargo test webhook".into(), output: Some("test result: ok. 6 passed".into()), failed: false }),
        timed(Entry::Agent {
            text: "**Recap.** Deliveries stop retrying after 30 s (`MAX_ELAPSED` in `src/webhooks/retry.rs:13`).\n\n**Plan**\n1. Raise the retry window to 15 minutes.\n2. Cap each delay at 2 minutes.\n3. Test the delay sequence.\n\n| File | Change | Risk |\n|---|---|---|\n| `src/webhooks/retry.rs` | cap each delay at 2 minutes | low |\n| `src/webhooks/mod.rs` | retry for 15 minutes | medium |\n| `tests/webhook_retry.rs` | delay sequence, attempt 0 | none |\n\n```rust\nconst MAX_DELAY: Duration = Duration::from_secs(120);\n```".into(),
        }),
        timed(Entry::TurnEnd { usage: Some(Usage { input_tokens: 41_200, output_tokens: 1_830, cost_usd: Some(0.42) }) }),
        timed(Entry::User { text: "Go ahead".into() }),
        timed(Entry::FromAgent {
            from: ConversationId(4),
            text: "Before you merge: add a test for attempt 0.".into(),
            answer: false,
        }),
        timed(Entry::FromAgent {
            from: ConversationId(2),
            text: "Done: the window is 15 minutes and each delay is capped at 2 minutes.".into(),
            answer: true,
        }),
        timed(Entry::Notice { text: "From your next message: model sonnet.".into() }),
        timed(Entry::Tool { id: "3".into(), name: "Edited".into(), detail: "src/webhooks/retry.rs".into(), output: None, failed: false }),
    ];
    let mut transcripts = HashMap::new();
    transcripts.insert(
        ConversationId(1),
        Arc::new(TranscriptView {
            entries: Arc::new(transcript.clone()),
            partial: Some("Editing the retry loop: the delay now".into()),
        }),
    );
    transcripts.insert(ConversationId(4), review());
    transcripts.insert(
        ConversationId(2),
        Arc::new(TranscriptView {
            entries: Arc::new(vec![
                timed(Entry::FromAgent {
                    from: ConversationId(1),
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
    helpers(&mut transcripts);
    transcripts.insert(
        ConversationId(3),
        Arc::new(TranscriptView {
            entries: Arc::new(transcript[..4].to_vec()),
            partial: None,
        }),
    );
    transcripts
}

/// The first reviewer: the planner's request, its review, then the user's
/// question and its answer, for the README's conversation picture.
fn review() -> Arc<TranscriptView> {
    let read = |id: &str, path: &str| {
        timed(Entry::Tool {
            id: id.into(),
            name: "Read".into(),
            detail: path.into(),
            output: Some("…".into()),
            failed: false,
        })
    };
    let usage = |input_tokens, output_tokens, cost| Entry::TurnEnd {
        usage: Some(Usage {
            input_tokens,
            output_tokens,
            cost_usd: Some(cost),
        }),
    };
    Arc::new(TranscriptView {
        entries: Arc::new(vec![
            timed(Entry::FromAgent {
                from: ConversationId(1),
                text: "Review the retry change in `src/webhooks/retry.rs`: the window is now 15 minutes and each delay is capped at 2 minutes.".into(),
                answer: false,
            }),
            read("r1", "src/webhooks/retry.rs"),
            timed(Entry::Tool {
                id: "r2".into(),
                name: "Ran".into(),
                detail: "cargo test webhook".into(),
                output: Some("test result: ok. 6 passed".into()),
                failed: false,
            }),
            timed(Entry::Agent {
                text: "**Review of the retry change**\n\nThe window and the cap are right, and the 6 webhook tests pass. Two findings:\n\n1. **No test for attempt 0.** The first retry calls `next_delay(0)`, which waits 1 s; nothing checks it.\n2. **`MAX_DELAY` is hard-coded** in `src/webhooks/retry.rs:14`. Fine for now; worth a setting if ops want 5 minutes.\n\nI would block on the first one only.".into(),
            }),
            timed(usage(18_400, 620, 0.11)),
            timed(Entry::User {
                text: "Is attempt 0 really reachable? I thought retries start at 1.".into(),
            }),
            read("r3", "src/webhooks/mod.rs"),
            timed(Entry::Agent {
                text: "Yes. The first retry passes the number of failures so far, which is 0 (`src/webhooks/mod.rs:41`):\n\n```rust\nlet delay = next_delay(failures); // 0 on the first retry\n```\n\nSo the first retry waits 2⁰ = 1 second. A test that pins it down:\n\n```rust\n#[test]\nfn first_retry_waits_one_second() {\n    assert_eq!(next_delay(0), Duration::from_secs(1));\n}\n```".into(),
            }),
            timed(usage(21_900, 310, 0.06)),
        ]),
        partial: None,
    })
}

/// The second coder and the tester of the first flow.
fn helpers(transcripts: &mut HashMap<ConversationId, Arc<TranscriptView>>) {
    let from = |id: i64, text: &str, answer: bool| {
        timed(Entry::FromAgent {
            from: ConversationId(id),
            text: text.into(),
            answer,
        })
    };
    let turn = |text: &str| {
        [
            timed(Entry::Agent { text: text.into() }),
            timed(Entry::TurnEnd { usage: None }),
        ]
    };
    let mut coder_2 = vec![from(1, "Write the tests for the capped delays.", false)];
    coder_2.extend(turn("Added 4 tests; all pass."));
    transcripts.insert(
        ConversationId(5),
        Arc::new(TranscriptView {
            entries: Arc::new(coder_2),
            partial: None,
        }),
    );
    let mut tester = vec![from(1, "Review the tests for the capped delays.", false)];
    tester.extend(turn("12 of 12 deliveries retried as expected."));
    tester.push(timed(Entry::User {
        text: "Also try a 20-minute outage.".into(),
    }));
    transcripts.insert(
        ConversationId(6),
        Arc::new(TranscriptView {
            entries: Arc::new(tester),
            partial: Some("Simulating a 20-minute outage".into()),
        }),
    );
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
                permissions: strings(&["auto", "bypassPermissions", "acceptEdits", "manual"]),
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
    let flows = vec![
        FlowView {
            // Unread: the reviewer's answer is not read yet.
            flow: flow(1, 1, "Fix webhook retry timeout", true, false),
            status: Status::Running,
            conversations: vec![
                ConversationId(1),
                ConversationId(2),
                ConversationId(4),
                ConversationId(5),
                ConversationId(6),
            ],
            folders: vec![repo::API.into(), repo::WEB.into()],
            issues: vec![
                leon_domain::GhRef::issue("acme/payments-api", 401),
                leon_domain::GhRef::issue("acme/web-dashboard", 31),
            ],
        },
        FlowView {
            flow: flow(2, 2, "Migrate config to TOML", true, false),
            status: Status::NeedsInput,
            conversations: vec![ConversationId(3)],
            folders: vec!["/code/infra-tools".into()],
            issues: vec![],
        },
        FlowView {
            flow: flow(3, 1, "Draft release notes v2.3", true, true),
            status: Status::Idle,
            conversations: vec![],
            folders: vec![],
            issues: vec![],
        },
        FlowView {
            flow: flow(4, 2, "Investigate flaky login e2e test", true, false),
            status: Status::Error,
            conversations: vec![],
            folders: vec![],
            issues: vec![],
        },
    ];
    let conversations = [
        conversation(1, 1, "planner", BackendKind::ClaudeCode, Status::Running),
        conversation(2, 1, "coder", BackendKind::Codex, Status::Idle),
        conversation(3, 2, "coder", BackendKind::Codex, Status::NeedsInput),
        conversation(4, 1, "reviewer", BackendKind::ClaudeCode, Status::Idle),
        conversation(5, 1, "coder", BackendKind::Codex, Status::Idle),
        conversation(6, 1, "reviewer", BackendKind::ClaudeCode, Status::Running),
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
                collapsed: false,
            },
        ]),
        flows: Arc::new(flows),
        conversations: Arc::new(
            conversations
                .into_iter()
                .map(|s| (s.conversation.id, s))
                .collect(),
        ),
        transcripts,
        agents: Arc::new(agents),
        options: Arc::new(options),
        git: repo::git(),
        prs: repo::prs(),
        linked_prs: repo::linked_prs(),
        usage: repo::usage(),
        my_issues: repo::my_issues(),
        my_prs: repo::my_prs(),
        todos: Arc::new(todos()),
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
        ("conversation".to_owned(), Shot::Conversation(FlowId(1))),
        // The README's conversation: you and the first reviewer.
        (
            "agent".to_owned(),
            Shot::Agent(FlowId(1), ConversationId(4)),
        ),
        ("question".to_owned(), Shot::Conversation(FlowId(2))),
        ("right-closed".to_owned(), Shot::RightClosed(FlowId(1))),
        ("slash-list".to_owned(), Shot::Typed(FlowId(1), "/dep")),
        (
            "slash-arguments".to_owned(),
            Shot::Typed(FlowId(1), "/deploy staging"),
        ),
        (
            "picker".to_owned(),
            Shot::PickerOpen(FlowId(1), "session-model"),
        ),
        ("flow".to_owned(), Shot::Flow(FlowId(1))),
        ("new-in-flow".to_owned(), Shot::NewInFlow(FlowId(1))),
        ("new-conversation".to_owned(), Shot::NewFlow),
        (
            "new-conversation-agent".to_owned(),
            Shot::NewFlowWith("coder"),
        ),
        ("right-branch".to_owned(), Shot::Right(FlowId(1), "branch")),
        ("right-info".to_owned(), Shot::Right(FlowId(1), "info")),
        ("review".to_owned(), Shot::Review(FlowId(1))),
        ("my-prs".to_owned(), Shot::MyPullRequests),
        ("my-issues".to_owned(), Shot::MyIssues),
        ("settings".to_owned(), Shot::Settings),
        ("doctor".to_owned(), Shot::Doctor),
        ("todos".to_owned(), Shot::Todos),
        // Drags under way: a project, then the first flow's to-dos, down
        // past the next ones and a little to the right.
        (
            "drag-project".to_owned(),
            Shot::Drag(Box::new(Shot::Todos), down_and_right(110., 291., 12.)),
        ),
        (
            "drag-todos".to_owned(),
            Shot::Drag(Box::new(Shot::Todos), down_and_right(380., 83., 14.)),
        ),
        ("row-menu".to_owned(), Shot::RowMenu(FlowId(1))),
        ("rename".to_owned(), Shot::Rename(FlowId(1))),
        (
            "edit-todo".to_owned(),
            Shot::EditTodo(leon_domain::TodoId(2)),
        ),
        ("tools".to_owned(), Shot::Tools),
        (
            "tools-asking".to_owned(),
            Shot::ToolsAsking(vec![leon_domain::BackendKind::ClaudeCode]),
        ),
        // Last: these leave the details drawer open.
        (
            "issue-detail".to_owned(),
            Shot::Detail(Box::new(Detail::Issue(Box::new(
                repo::my_issues()[0].clone(),
            )))),
        ),
        (
            "pr-diff".to_owned(),
            Shot::PrDiff(
                Box::new(Detail::PullRequest(Box::new(
                    repo::prs()[std::path::Path::new(repo::API)].clone(),
                ))),
                Box::new(leon_core::PrDiff {
                    pr: leon_domain::GhRef::pull_request("acme/payments-api", 412),
                    files: Some(Ok(repo::git()[std::path::Path::new(repo::API)]
                        .branch_diff
                        .clone())),
                }),
            ),
        ),
        (
            "pr-detail".to_owned(),
            Shot::Detail(Box::new(Detail::PullRequest(Box::new(
                repo::prs()[std::path::Path::new(repo::API)].clone(),
            )))),
        ),
        (
            "drawer".to_owned(),
            Shot::Drawer(FlowId(1), "src/webhooks/retry.rs", false),
        ),
        (
            "drawer-split".to_owned(),
            Shot::Drawer(FlowId(1), "src/webhooks/retry.rs", true),
        ),
    ];
    gpui_platform::application()
        .with_assets(leon_ui::Assets)
        .run(move |cx| capture(&core, shots, out, cx));
}
