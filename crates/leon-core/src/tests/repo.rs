//! Git and GitHub state of a conversation.

use std::sync::Mutex;

use leon_domain::{FileDiff, FileStatus, Issue, PrState, PullRequest};

use super::*;
use crate::ports::{Git, GitHub};

pub(super) struct FakeGit;

impl Git for FakeGit {
    fn github_repo(&self, _dir: &Path) -> Option<String> {
        Some("acme/api".into())
    }
    fn doctor(&self) -> leon_domain::DoctorSection {
        leon_domain::DoctorSection {
            title: "git".into(),
            checks: vec![],
        }
    }
    fn branch(&self, _dir: &Path) -> Option<String> {
        Some("fix-retry".into())
    }
    fn default_base(&self, _dir: &Path) -> Option<String> {
        Some("origin/main".into())
    }
    fn changes(&self, _dir: &Path) -> Result<Vec<FileDiff>, PortError> {
        Ok(vec![file("src/retry.rs")])
    }
    fn branch_diff(&self, _dir: &Path, base: &str) -> Result<Vec<FileDiff>, PortError> {
        assert_eq!(base, "origin/main");
        Ok(vec![file("src/retry.rs"), file("Cargo.lock")])
    }
}

fn file(path: &str) -> FileDiff {
    FileDiff {
        path: path.into(),
        status: FileStatus::Modified,
        added: 1,
        removed: 0,
        binary: false,
        hunks: vec![],
    }
}

#[derive(Default)]
pub(super) struct FakeGitHub {
    reviews: Mutex<Vec<u64>>,
}

impl GitHub for FakeGitHub {
    fn doctor(&self) -> leon_domain::DoctorSection {
        leon_domain::DoctorSection {
            title: "GitHub CLI".into(),
            checks: vec![],
        }
    }
    fn pull_request(&self, _dir: &Path, branch: &str) -> Result<Option<PullRequest>, PortError> {
        Ok((branch == "fix-retry").then(|| PullRequest {
            repo: "acme/api".into(),
            number: 7,
            title: "Fix retry".into(),
            url: String::new(),
            state: PrState::Open,
            branch: branch.into(),
            base: "main".into(),
            labels: vec![],
            checks: vec![],
            rollup: None,
            reviewers: vec![],
            comments: vec![],
        }))
    }
    fn my_issues(&self) -> Result<Vec<Issue>, PortError> {
        Err(PortError::new("listing my issues", "gh is not logged in"))
    }
    fn my_pull_requests(&self) -> Result<Vec<PullRequest>, PortError> {
        Ok(vec![])
    }
    fn request_copilot_review(&self, _repo: &str, number: u64) -> Result<(), PortError> {
        self.reviews.lock().unwrap().push(number);
        Ok(())
    }
    fn comment(&self, _repo: &str, _number: u64, _body: &str) -> Result<(), PortError> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_shows_its_branch_changes_and_pull_request() {
    let h = harness();
    new_conversation(&h, BackendKind::Codex).await;
    let id = until(&h.core, "conversation", |s| !s.conversations.is_empty())
        .await
        .conversations[0]
        .conversation
        .id;
    h.core.send(Intent::View(Some(id)));
    let s = until(&h.core, "pull request", |s| s.prs.contains_key(&id)).await;
    let git = &s.git[&id];
    assert_eq!(git.branch.as_deref(), Some("fix-retry"));
    assert_eq!(git.changes.len(), 1);
    assert_eq!(git.branch_diff.len(), 2);
    assert_eq!(s.prs[&id].number, 7);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_github_failure_is_shown_not_fatal() {
    let h = harness();
    let s = until(&h.core, "github error", |s| s.github_error.is_some()).await;
    assert!(
        s.github_error
            .as_deref()
            .is_some_and(|e| e.contains("not logged in"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn requesting_a_copilot_review_says_so() {
    let h = harness();
    new_conversation(&h, BackendKind::Codex).await;
    let id = until(&h.core, "conversation", |s| !s.conversations.is_empty())
        .await
        .conversations[0]
        .conversation
        .id;
    h.core.send(Intent::View(Some(id)));
    until(&h.core, "pull request", |s| s.prs.contains_key(&id)).await;
    h.core.send(Intent::RequestCopilotReview(id));
    let s = until(&h.core, "flash", |s| s.flash.is_some()).await;
    assert_eq!(
        s.flash.as_ref().map(|f| f.1.as_str()),
        Some("Copilot review requested on #7")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_doctor_runs_at_start_with_every_section() {
    let h = harness();
    let s = until(&h.core, "doctor", |s| s.doctor.is_some()).await;
    let (_, sections) = s.doctor.clone().unwrap();
    let titles: Vec<&str> = sections.iter().map(|x| x.title.as_str()).collect();
    assert_eq!(titles, ["Leon", "Codex", "git", "GitHub CLI"]);
    let leon: Vec<&str> = sections[0].checks.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(leon, ["Version", "Tools for agents"]);
    assert_eq!(sections[1].health(), leon_domain::Health::Problem);
    assert!(!s.doctor_running);
}
