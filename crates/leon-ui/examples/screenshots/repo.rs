//! Sample git and GitHub state for the screenshots.

use std::collections::HashMap;
use std::sync::Arc;

use leon_core::GitView;
use leon_domain::{
    BoardOption, Check, CheckState, Comment, ConversationId, Day, DiffLine, FileDiff, FileStatus,
    FoldPattern, Hunk, Issue, Iteration, Label, LineKind, PrState, PullRequest, ReviewThread,
};

fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
    DiffLine {
        kind,
        old,
        new,
        text: text.into(),
    }
}

fn retry_rs() -> FileDiff {
    use LineKind::{Added, Context, Removed};
    FileDiff {
        path: "src/webhooks/retry.rs".into(),
        status: FileStatus::Modified,
        added: 3,
        removed: 1,
        binary: false,
        hunks: vec![Hunk {
            header: "@@ -10,7 +10,9 @@ use crate::webhooks::Delivery;".into(),
            lines: vec![
                line(Context, Some(10), Some(10), "use std::time::Duration;"),
                line(Context, Some(11), Some(11), ""),
                line(
                    Removed,
                    Some(12),
                    None,
                    "const MAX_ELAPSED: Duration = Duration::from_secs(30);",
                ),
                line(
                    Added,
                    None,
                    Some(12),
                    "const MAX_ELAPSED: Duration = Duration::from_secs(15 * 60);",
                ),
                line(
                    Added,
                    None,
                    Some(13),
                    "/// Longest wait between two attempts.",
                ),
                line(
                    Added,
                    None,
                    Some(14),
                    "const MAX_DELAY: Duration = Duration::from_secs(120);",
                ),
                line(Context, Some(13), Some(15), ""),
                line(
                    Context,
                    Some(14),
                    Some(16),
                    "pub fn next_delay(attempt: u32, elapsed: Duration) -> Option<Duration> {",
                ),
            ],
        }],
    }
}

fn file(path: &str, status: FileStatus, added: u32, removed: u32) -> FileDiff {
    FileDiff {
        path: path.into(),
        status,
        added,
        removed,
        binary: false,
        hunks: vec![Hunk {
            header: "@@ -1201,7 +1201,7 @@".into(),
            lines: vec![line(
                LineKind::Context,
                Some(1201),
                Some(1201),
                "name = \"serde\"",
            )],
        }],
    }
}

pub fn git() -> Arc<HashMap<ConversationId, GitView>> {
    Arc::new(HashMap::from([(
        ConversationId(1),
        GitView {
            branch: Some("fix-webhook-retry".into()),
            base: Some("origin/main".into()),
            changes: Arc::new(vec![
                file("Cargo.lock", FileStatus::Modified, 1, 1),
                retry_rs(),
            ]),
            branch_diff: Arc::new(vec![
                retry_rs(),
                file("tests/webhook_retry.rs", FileStatus::Added, 17, 0),
            ]),
            error: None,
        },
    )]))
}

pub fn fold() -> Arc<Vec<FoldPattern>> {
    Arc::new(FoldPattern::parse("glob:**/*.lock").into_iter().collect())
}

fn pr(number: u64, title: &str, state: PrState) -> PullRequest {
    PullRequest {
        repo: "acme/payments-api".into(),
        number,
        title: title.into(),
        url: String::new(),
        state,
        branch: "fix-webhook-retry".into(),
        base: "main".into(),
        labels: vec![label("bug", "d73a4a"), label("payments", "0e8a16")],
        rollup: None,
        checks: vec![
            Check {
                name: "build".into(),
                state: CheckState::Pass,
            },
            Check {
                name: "test (ubuntu)".into(),
                state: CheckState::Pass,
            },
            Check {
                name: "test (macos)".into(),
                state: CheckState::Fail,
            },
        ],
        reviewers: vec!["copilot-pull-request-reviewer".into()],
        comments: vec![Comment {
            author: "copilot-pull-request-reviewer".into(),
            body: "The new `MAX_DELAY` cap looks right. Consider a test for attempt 0.".into(),
            at: crate::hours_ago(1),
        }],
        body: "Webhook deliveries now retry for 15 minutes, each delay capped at 2 minutes.\n\n\
               <details><summary>Test results: 42 passed</summary>\n\n\
               - retry::caps_the_delay ok\n- retry::gives_up_after_15_minutes ok\n\
               </details>"
            .into(),
        threads: vec![ReviewThread {
            path: "src/webhooks/retry.rs".into(),
            line: Some(14),
            comments: vec![Comment {
                author: "reviewer-a".into(),
                body: "Should `MAX_DELAY` come from the config?".into(),
                at: crate::hours_ago(2),
            }],
        }],
    }
}

pub fn prs() -> Arc<HashMap<ConversationId, PullRequest>> {
    Arc::new(HashMap::from([(
        ConversationId(1),
        pr(412, "Fix webhook retry timeout", PrState::Open),
    )]))
}

fn label(name: &str, color: &str) -> Label {
    Label {
        name: name.into(),
        color: color.into(),
    }
}

/// A list PR: checks summed up by GitHub, as the search returns them.
fn listed(number: u64, title: &str, state: PrState, rollup: CheckState) -> PullRequest {
    PullRequest {
        checks: vec![],
        rollup: Some(rollup),
        comments: vec![],
        ..pr(number, title, state)
    }
}

pub fn my_prs() -> Arc<Vec<PullRequest>> {
    Arc::new(vec![
        listed(
            412,
            "Fix webhook retry timeout",
            PrState::Open,
            CheckState::Pending,
        ),
        PullRequest {
            branch: "csv-export".into(),
            labels: vec![label("feature", "a2eeef")],
            ..listed(
                398,
                "Export invoices as CSV",
                PrState::Draft,
                CheckState::Fail,
            )
        },
        PullRequest {
            repo: "acme/web".into(),
            branch: "bump-deps".into(),
            labels: vec![label("dependencies", "0366d6")],
            ..listed(77, "Bump the date library", PrState::Open, CheckState::Pass)
        },
    ])
}

pub fn my_issues() -> Arc<Vec<Issue>> {
    let status = |name: &str, order| {
        Some(BoardOption {
            name: name.into(),
            order,
            color: None,
        })
    };
    let priority = |name: &str, order, color: &str| {
        Some(BoardOption {
            name: name.into(),
            order,
            color: Some(color.into()),
        })
    };
    // The current sprint started two days ago; the one before ended then.
    let today = Day::from_unix_ms(crate::hours_ago(0));
    let sprint = |title: &str, started_days_ago: i64| {
        vec![Iteration {
            field: "Sprint".into(),
            title: title.into(),
            start: today.plus(-started_days_ago),
            days: 14,
        }]
    };
    let issue = |number, title: &str, labels, status, hours| Issue {
        repo: "acme/payments-api".into(),
        number,
        title: String::from(title),
        url: String::new(),
        labels,
        updated_at: crate::hours_ago(hours),
        status,
        priority: None,
        iterations: Vec::new(),
        assignees: vec!["octocat".into()],
        body: "Deliveries stop retrying after 30 s, so a slow endpoint loses events.\n\n\
               **Expected:** retries for 15 minutes, each delay capped at 2 minutes."
            .into(),
        comments: Vec::new(),
    };
    Arc::new(vec![
        Issue {
            priority: priority("P1", 1, "ORANGE"),
            iterations: sprint("Sprint 12", 2),
            ..issue(
                401,
                "Webhook retries give up after 30 seconds",
                vec![label("bug", "d73a4a")],
                status("In progress", 2),
                3,
            )
        },
        Issue {
            priority: priority("P2", 2, "YELLOW"),
            iterations: sprint("Sprint 12", 2),
            ..issue(
                405,
                "Refunds over 10k need a second approval",
                vec![label("feature", "a2eeef"), label("payments", "0e8a16")],
                status("Todo", 1),
                20,
            )
        },
        Issue {
            priority: priority("P0", 0, "RED"),
            iterations: sprint("Sprint 12", 2),
            ..issue(
                388,
                "Invoice PDF misses the VAT number",
                vec![label("bug", "d73a4a")],
                status("Todo", 1),
                50,
            )
        },
        Issue {
            iterations: sprint("Sprint 11", 16),
            ..issue(
                392,
                "Retry dashboard shows UTC times",
                vec![],
                status("In review", 3),
                5,
            )
        },
        issue(
            379,
            "Move the cron jobs to the new scheduler",
            vec![],
            status("Backlog", 0),
            200,
        ),
        issue(
            366,
            "Document the webhook signature",
            vec![label("docs", "0075ca")],
            None,
            400,
        ),
    ])
}

pub fn doctor() -> (i64, Arc<Vec<leon_domain::DoctorSection>>) {
    use leon_domain::{DoctorCheck as C, DoctorSection as S, Health as H};
    let sections = vec![
        S {
            title: "Leon".into(),
            checks: vec![
                C::new(H::Info, "Version", "Leon 0.1.0 (macos, aarch64)"),
                C::new(H::Ok, "Config", "~/.config/leon/config.toml"),
                C::new(H::Info, "Data folder", "~/Library/Application Support/leon"),
                C::new(H::Ok, "PATH", "read from your login shell"),
                C::new(
                    H::Warning,
                    "Notifications",
                    "off: Leon is not running as an app (Leon.app)",
                )
                .fix("Run Leon from its app bundle to get desktop notifications"),
                C::new(
                    H::Ok,
                    "Tools for agents",
                    "listening on ~/Library/Application Support/leon/tools.sock",
                ),
                C::new(
                    H::Info,
                    "Source folders",
                    "none (`[sources] folders` in the config)",
                ),
            ],
        },
        S {
            title: "Claude Code".into(),
            checks: vec![
                C::new(
                    H::Ok,
                    "Version",
                    "2.1.288 (Claude Code) (Leon was checked with 2.1.288)",
                ),
                C::new(H::Problem, "Logged in", "no").fix("Run `claude auth login` in a terminal"),
            ],
        },
        S {
            title: "Codex".into(),
            checks: vec![
                C::new(
                    H::Ok,
                    "Version",
                    "codex-cli 0.160.0 (Leon was checked with 0.160.0)",
                ),
                C::new(H::Ok, "Logged in", "yes"),
            ],
        },
        S {
            title: "Copilot CLI".into(),
            checks: vec![C::new(H::Problem, "Installed", "`copilot` not found").fix(
                "Install Copilot CLI so that `copilot` is on your PATH, then run the doctor again",
            )],
        },
        S {
            title: "git".into(),
            checks: vec![C::new(
                H::Ok,
                "Installed",
                "git version 2.54.0 (Apple Git-157)",
            )],
        },
        S {
            title: "GitHub CLI".into(),
            checks: vec![
                C::new(
                    H::Ok,
                    "Version",
                    "gh version 2.101.0 (2026-09-15) (Leon was checked with 2.101.0)",
                ),
                C::new(H::Ok, "Logged in", "yes"),
            ],
        },
    ];
    (crate::hours_ago(0), Arc::new(sections))
}

pub fn shared_tools() -> Arc<Vec<leon_domain::McpServer>> {
    Arc::new(vec![leon_domain::McpServer {
        name: "metabase".into(),
        command: "npx".into(),
        args: vec!["-y".into(), "@org/metabase-mcp".into()],
        env: vec![
            ("METABASE_URL".into(), "https://example.invalid".into()),
            ("API_KEY".into(), "x".into()),
        ],
        trusted: false,
    }])
}

pub fn tools() -> (i64, Arc<Vec<leon_domain::CliConnectors>>) {
    use leon_domain::{
        BackendKind as B, CliConnectors as Cli, Connector as C, ConnectorKind as K,
        ConnectorState as S,
    };
    let c = |name: &str, kind, state, detail: &str| C {
        name: name.into(),
        kind,
        state,
        detail: detail.into(),
    };
    let clis = vec![
        Cli {
            backend: B::ClaudeCode,
            connectors: vec![
                c("Slack", K::Account, S::On, "https://mcp.slack.com/mcp"),
                c(
                    "Gmail",
                    K::Account,
                    S::On,
                    "https://gmail.mcp.claude.com/mcp",
                ),
                c(
                    "Metabase",
                    K::Account,
                    S::On,
                    "https://example.metabaseapp.com/api/mcp",
                ),
                c("Asana", K::Account, S::Failed, "HTTP 410"),
                c(
                    "Atlassian",
                    K::Account,
                    S::NeedsLogin,
                    "https://mcp.atlassian.com/v1/mcp",
                ),
                c(
                    "Figma",
                    K::Account,
                    S::NeedsLogin,
                    "https://mcp.figma.com/mcp",
                ),
                c("team-kit@team-marketplace", K::Plugin, S::On, "1.4.0"),
            ],
            problem: None,
        },
        Cli {
            backend: B::Codex,
            connectors: vec![
                c("github@openai-curated", K::Plugin, S::On, "0.3.1"),
                c("node_repl", K::Server, S::On, "node"),
                c("codex_app", K::Server, S::Off, "codex-app"),
            ],
            problem: None,
        },
        Cli {
            backend: B::CopilotCli,
            connectors: vec![c("computer-use", K::Plugin, S::Off, "built-in")],
            problem: None,
        },
    ];
    (crate::hours_ago(0), Arc::new(clis))
}
