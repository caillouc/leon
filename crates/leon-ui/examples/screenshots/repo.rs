//! Sample git and GitHub state for the screenshots.

use std::collections::HashMap;
use std::sync::Arc;

use leon_core::GitView;
use leon_domain::{
    Check, CheckState, Comment, ConversationId, DiffLine, FileDiff, FileStatus, FoldPattern, Hunk,
    Issue, LineKind, PrState, PullRequest,
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
        labels: vec!["bug".into(), "payments".into()],
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
                state: CheckState::Pending,
            },
        ],
        reviewers: vec!["copilot-pull-request-reviewer".into()],
        comments: vec![Comment {
            author: "copilot-pull-request-reviewer".into(),
            body: "The new `MAX_DELAY` cap looks right. Consider a test for attempt 0.".into(),
            at: 0,
        }],
    }
}

pub fn prs() -> Arc<HashMap<ConversationId, PullRequest>> {
    Arc::new(HashMap::from([(
        ConversationId(1),
        pr(412, "Fix webhook retry timeout", PrState::Open),
    )]))
}

pub fn my_prs() -> Arc<Vec<PullRequest>> {
    Arc::new(vec![
        pr(412, "Fix webhook retry timeout", PrState::Open),
        pr(398, "Export invoices as CSV", PrState::Draft),
    ])
}

pub fn my_issues() -> Arc<Vec<Issue>> {
    Arc::new(vec![Issue {
        repo: "acme/payments-api".into(),
        number: 401,
        title: "Webhook retries give up after 30 seconds".into(),
        url: String::new(),
        labels: vec!["bug".into()],
        updated_at: 0,
    }])
}
