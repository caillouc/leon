//! Git and GitHub state of a conversation.

use std::sync::Mutex;

use leon_domain::{FileDiff, FileStatus, GhRef, Issue, PrState, PullRequest};

use crate::Detail;

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
    fn branch(&self, dir: &Path) -> Option<String> {
        Some(
            if dir.starts_with("/code/web") {
                "web-fix"
            } else {
                "fix-retry"
            }
            .into(),
        )
    }
    /// `/code/api` and `/code/web` are repositories, `/code/api/.worktrees/fix`
    /// a worktree of the first.
    fn working_tree(&self, path: &Path) -> Option<PathBuf> {
        ["/code/api/.worktrees/fix", "/code/api", "/code/web"]
            .into_iter()
            .find(|root| path.starts_with(root))
            .map(PathBuf::from)
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
        let (repo, number, closes) = match branch {
            "fix-retry" => ("acme/api", 7, GhRef::issue("acme/api", 12)),
            "web-fix" => ("acme/web", 3, GhRef::issue("acme/web", 5)),
            _ => return Ok(None),
        };
        Ok(Some(PullRequest {
            repo: repo.into(),
            number,
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
            body: String::new(),
            threads: vec![],
            author: String::new(),
            closes: vec![closes],
        }))
    }
    fn my_issues(&self) -> Result<Vec<Issue>, PortError> {
        Err(PortError::new("listing my issues", "gh is not logged in"))
    }
    fn my_pull_requests(&self) -> Result<Vec<PullRequest>, PortError> {
        Ok(vec![])
    }
    fn pull_request_by_number(&self, repo: &str, number: u64) -> Result<PullRequest, PortError> {
        let pr = self.pull_request(Path::new("/code/api"), "fix-retry")?;
        pr.filter(|pr| pr.repo == repo && pr.number == number)
            .ok_or_else(|| PortError::new("reading a pull request", "not found"))
    }
    fn issue(&self, repo: &str, number: u64) -> Result<Issue, PortError> {
        Ok(Issue {
            repo: repo.into(),
            number,
            title: "Retries give up".into(),
            url: String::new(),
            labels: vec![],
            updated_at: 0,
            status: None,
            priority: None,
            iterations: vec![],
            assignees: vec!["octocat".into()],
            body: "They stop after 30 s.".into(),
            comments: vec![],
        })
    }
    fn request_copilot_review(&self, _repo: &str, number: u64) -> Result<(), PortError> {
        self.reviews.lock().unwrap().push(number);
        Ok(())
    }
    fn comment(&self, _repo: &str, _number: u64, _body: &str) -> Result<(), PortError> {
        Ok(())
    }
    fn set_draft(&self, _repo: &str, number: u64, _draft: bool) -> Result<(), PortError> {
        self.reviews.lock().unwrap().push(number);
        Ok(())
    }
    fn reply_to_thread(&self, thread: &str, _body: &str) -> Result<(), PortError> {
        Err(PortError::new(
            "answering a review thread",
            format!("{thread} is gone"),
        ))
    }
    fn resolve_thread(&self, _thread: &str) -> Result<(), PortError> {
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
    let api = Path::new("/code/api");
    let s = until(&h.core, "pull request", |s| s.prs.contains_key(api)).await;
    let git = &s.git[api];
    assert_eq!(git.branch.as_deref(), Some("fix-retry"));
    assert_eq!(git.changes.len(), 1);
    assert_eq!(git.branch_diff.len(), 2);
    assert_eq!(s.prs[api].number, 7);
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
    until(&h.core, "pull request", |s| {
        s.prs.contains_key(Path::new("/code/api"))
    })
    .await;
    let pr = GhRef::pull_request("acme/api", 7);
    // Each action says how it went, failures included (GH-5, GH-8).
    let cases = [
        (
            Intent::RequestCopilotReview(pr.clone()),
            "Copilot review requested on #7",
        ),
        (
            Intent::ResolveThread {
                pr: pr.clone(),
                thread: "PRRT_1".into(),
            },
            "Thread resolved",
        ),
        (
            Intent::ReplyToThread {
                pr: pr.clone(),
                thread: "PRRT_2".into(),
                body: "Done.".into(),
            },
            "Could not answer the thread: answering a review thread: PRRT_2 is gone",
        ),
    ];
    for (intent, said) in cases {
        h.core.send(intent);
        until(&h.core, said, |s| {
            s.flash.as_ref().is_some_and(|f| f.1 == said)
        })
        .await;
    }
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

#[tokio::test(flavor = "multi_thread")]
async fn details_load_in_the_background_and_close() {
    let h = harness();
    until(&h.core, "loaded", |s| s.loaded).await;
    let item = GhRef::issue("acme/api", 12);
    h.core.send(Intent::ShowDetail(Some(item.clone())));
    let s = until(&h.core, "issue", |s| {
        matches!(&s.detail, Some(Detail::Issue(_)))
    })
    .await;
    let Some(Detail::Issue(issue)) = &s.detail else {
        unreachable!()
    };
    assert_eq!(
        (issue.number, issue.body.as_str()),
        (12, "They stop after 30 s.")
    );

    h.core.send(Intent::ShowDetail(Some(GhRef::pull_request(
        "acme/api", 99,
    ))));
    let s = until(&h.core, "failure", |s| {
        matches!(&s.detail, Some(Detail::Failed(..)))
    })
    .await;
    assert_eq!(
        s.detail.as_ref().map(Detail::item),
        Some(GhRef::pull_request("acme/api", 99))
    );
    h.core.send(Intent::ShowDetail(None));
    until(&h.core, "closed", |s| s.detail.is_none()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pull_request_goes_ready_or_back_to_draft() {
    let h = harness();
    until(&h.core, "loaded", |s| s.loaded).await;
    h.core
        .send(Intent::SetDraft(GhRef::pull_request("acme/api", 7), false));
    let s = until(&h.core, "flash", |s| s.flash.is_some()).await;
    assert_eq!(
        s.flash.as_ref().map(|f| f.1.as_str()),
        Some("#7 is ready for review")
    );
}
