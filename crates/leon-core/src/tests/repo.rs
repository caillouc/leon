//! Git and GitHub state of a flow.

use std::sync::Mutex;

use leon_domain::{FileDiff, FileStatus, GhRef, Issue, PrState, PullRequest};

use crate::Detail;

use super::*;
use crate::ports::{Git, GitHub};

#[derive(Default)]
pub(super) struct FakeGit {
    /// `(folder, branch)` of each fetch, in order.
    pub fetches: Mutex<Vec<(PathBuf, String)>>,
}

impl Git for FakeGit {
    fn fetch(&self, dir: &Path, branch: &str) -> Result<(), PortError> {
        self.fetches
            .lock()
            .unwrap()
            .push((dir.to_owned(), branch.to_owned()));
        Ok(())
    }
    fn github_repo(&self, _dir: &Path) -> Option<String> {
        Some("acme/api".into())
    }
    fn doctor(&self) -> leon_domain::DoctorSection {
        leon_domain::DoctorSection {
            title: "git".into(),
            checks: vec![],
        }
    }
    /// `/code/docs` sits on its default branch, `main`.
    fn branch(&self, dir: &Path) -> Option<String> {
        Some(
            if dir.starts_with("/code/web") {
                "web-fix"
            } else if dir.starts_with("/code/docs") {
                "main"
            } else {
                "fix-retry"
            }
            .into(),
        )
    }
    /// Every folder but a worktree under `.worktrees` is a repository's own.
    fn is_main_checkout(&self, dir: &Path) -> bool {
        !dir.components().any(|c| c.as_os_str() == ".worktrees")
    }
    /// `/code/api` and `/code/web` are repositories, `/code/api/.worktrees/fix`
    /// a worktree of the first.
    fn working_tree(&self, path: &Path) -> Option<PathBuf> {
        [
            "/code/api/.worktrees/fix",
            "/code/api",
            "/code/web",
            "/code/docs",
        ]
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
    /// `origin/main` is behind `origin/master`: since `main`, the branch
    /// diff holds one more file.
    fn branch_diff(&self, _dir: &Path, base: &str) -> Result<Vec<FileDiff>, PortError> {
        match base {
            "origin/main" => Ok(vec![file("src/retry.rs"), file("Cargo.lock")]),
            "origin/master" => Ok(vec![file("src/app.ts")]),
            _ => Err(PortError::new(
                "reading the branch diff",
                "unknown revision",
            )),
        }
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
        // The web pull request goes straight to `master`.
        let (repo, number, closes, base) = match branch {
            "fix-retry" => ("acme/api", 7, GhRef::issue("acme/api", 12), "main"),
            "web-fix" => ("acme/web", 3, GhRef::issue("acme/web", 5), "master"),
            _ => return Ok(None),
        };
        Ok(Some(PullRequest {
            repo: repo.into(),
            number,
            title: "Fix retry".into(),
            url: String::new(),
            state: PrState::Open,
            branch: branch.into(),
            base: base.into(),
            labels: vec![],
            checks: vec![],
            rollup: None,
            reviewers: vec![],
            comments: vec![],
            body: String::new(),
            threads: vec![],
            author: String::new(),
            closes: vec![closes],
            lines: None,
            conflicts: false,
        }))
    }
    fn my_issues(&self) -> Result<Vec<Issue>, PortError> {
        Err(PortError::new("listing my issues", "gh is not logged in"))
    }
    fn my_pull_requests(&self) -> Result<Vec<PullRequest>, PortError> {
        Ok(vec![])
    }
    /// Read by its number, the pull request is newer than by its branch:
    /// its checks have run since.
    fn pull_request_by_number(&self, repo: &str, number: u64) -> Result<PullRequest, PortError> {
        let pr = self.pull_request(Path::new("/code/api"), "fix-retry")?;
        pr.filter(|pr| pr.repo == repo && pr.number == number)
            .map(|pr| PullRequest {
                checks: vec![leon_domain::Check {
                    name: "test".into(),
                    state: leon_domain::CheckState::Pass,
                    url: None,
                }],
                ..pr
            })
            .ok_or_else(|| PortError::new("reading a pull request", "not found"))
    }
    fn pull_request_diff(&self, _repo: &str, number: u64) -> Result<Vec<FileDiff>, PortError> {
        if number == 404 {
            return Err(PortError::new(
                "reading the pull request's changes",
                "not found",
            ));
        }
        Ok(vec![file("src/retry.rs")])
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
    /// `#99` answers slowly, as GitHub may.
    fn set_draft(&self, _repo: &str, number: u64, _draft: bool) -> Result<(), PortError> {
        if number == 99 {
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        self.reviews.lock().unwrap().push(number);
        Ok(())
    }
    /// `PRRT_2` is gone; the other threads take answers.
    fn reply_to_thread(&self, thread: &str, _body: &str) -> Result<(), PortError> {
        if thread == "PRRT_2" {
            return Err(PortError::new(
                "answering a review thread",
                format!("{thread} is gone"),
            ));
        }
        Ok(())
    }
    fn resolve_thread(&self, _thread: &str) -> Result<(), PortError> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_flow_shows_its_branch_changes_and_pull_request() {
    let h = harness();
    new_flow(&h, BackendKind::Codex).await;
    let id = until(&h.core, "conversation", |s| !s.flows.is_empty())
        .await
        .flows[0]
        .flow
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

/// A pull request opened in its details replaces every copy Leon shows of
/// it: the flow's Info no longer lags behind the details (CTX-1, GH-3).
#[tokio::test(flavor = "multi_thread")]
async fn an_opened_pull_request_refreshes_every_copy_of_it() {
    let h = harness();
    new_flow(&h, BackendKind::Codex).await;
    let id = until(&h.core, "conversation", |s| !s.flows.is_empty())
        .await
        .flows[0]
        .flow
        .id;
    h.core.send(Intent::View(Some(id)));
    let api = Path::new("/code/api");
    let s = until(&h.core, "pull request", |s| s.prs.contains_key(api)).await;
    assert_eq!(s.prs[api].checks, vec![]);
    h.core
        .send(Intent::ShowDetail(Some(GhRef::pull_request("acme/api", 7))));
    let s = until(&h.core, "fresh checks", |s| {
        s.prs.get(api).is_some_and(|pr| !pr.checks.is_empty())
    })
    .await;
    assert_eq!(s.prs[api].checks[0].state, leon_domain::CheckState::Pass);
}

/// Looking at a flow fetches the base of its pull request once, not again
/// within five minutes (GIT-3, I22).
#[tokio::test(flavor = "multi_thread")]
async fn a_flows_base_is_fetched_when_looked_at() {
    let h = harness();
    new_flow(&h, BackendKind::Codex).await;
    let id = until(&h.core, "conversation", |s| !s.flows.is_empty())
        .await
        .flows[0]
        .flow
        .id;
    let api = Path::new("/code/api");
    h.core.send(Intent::View(Some(id)));
    until(&h.core, "pull request", |s| s.prs.contains_key(api)).await;
    h.core.send(Intent::RefreshGit(id));
    let fetched = || h.git.fetches.lock().unwrap().clone();
    for _ in 0..200 {
        if !fetched().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    // Again: once both are handled, and a fetch would have had time, none.
    h.core.send(Intent::RefreshGit(id));
    h.core.send(Intent::View(Some(id)));
    h.core.send(Intent::MarkUnread(id));
    until(&h.core, "handled", |s| s.flows[0].flow.unread).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(fetched(), [(api.to_owned(), "main".to_owned())]);
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

/// A pull request's changes are read from GitHub for the diff drawer, and
/// a failure says why (GH-3).
#[tokio::test(flavor = "multi_thread")]
async fn a_pull_requests_changes_come_from_github() {
    let h = harness();
    until(&h.core, "loaded", |s| s.loaded).await;
    h.core
        .send(Intent::ShowPrDiff(Some(GhRef::pull_request("acme/api", 7))));
    let s = until(&h.core, "the changes", |s| {
        s.pr_diff.as_ref().is_some_and(|d| d.files.is_some())
    })
    .await;
    let diff = s.pr_diff.clone().unwrap();
    let files = diff.files.unwrap().unwrap();
    assert_eq!(
        files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        ["src/retry.rs"]
    );
    h.core.send(Intent::ShowPrDiff(Some(GhRef::pull_request(
        "acme/api", 404,
    ))));
    let s = until(&h.core, "the failure", |s| {
        s.pr_diff
            .as_ref()
            .is_some_and(|d| matches!(&d.files, Some(Err(e)) if e.contains("not found")))
    })
    .await;
    assert_eq!(s.pr_diff.as_ref().map(|d| d.pr.number), Some(404));
    h.core.send(Intent::ShowPrDiff(None));
    until(&h.core, "closed", |s| s.pr_diff.is_none()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn requesting_a_copilot_review_says_so() {
    let h = harness();
    new_flow(&h, BackendKind::Codex).await;
    let id = until(&h.core, "conversation", |s| !s.flows.is_empty())
        .await
        .flows[0]
        .flow
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
        // Resolving answers "Fixed" first; an answer that fails resolves
        // nothing.
        (
            Intent::ResolveThread {
                pr: pr.clone(),
                thread: "PRRT_1".into(),
            },
            "Resolved as fixed",
        ),
        (
            Intent::ResolveThread {
                pr: pr.clone(),
                thread: "PRRT_2".into(),
            },
            "Could not resolve the thread: answering a review thread: PRRT_2 is gone",
        ),
        (
            Intent::ReplyToThread {
                pr: pr.clone(),
                thread: "PRRT_2".into(),
                body: "Done.".into(),
                resolve: true,
            },
            "Could not answer the thread: answering a review thread: PRRT_2 is gone",
        ),
        (
            Intent::ReplyToThread {
                pr: pr.clone(),
                thread: "PRRT_3".into(),
                body: "Done.".into(),
                resolve: false,
            },
            "Answer posted",
        ),
        (
            Intent::ReplyToThread {
                pr: pr.clone(),
                thread: "PRRT_4".into(),
                body: "Fixed.".into(),
                resolve: true,
            },
            "Answered and resolved",
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
async fn the_doctor_runs_when_asked_with_every_section() {
    let h = harness();
    until(&h.core, "loaded", |s| s.loaded).await;
    assert!(h.core.snapshot().doctor.is_none());
    h.core.send(Intent::RunDoctor);
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

/// A GitHub action shows under way from its click until GitHub answers,
/// then its notice says how it went (GH-7).
#[tokio::test(flavor = "multi_thread")]
async fn an_action_shows_under_way_until_github_answers() {
    let h = harness();
    until(&h.core, "loaded", |s| s.loaded).await;
    let pr = GhRef::pull_request("acme/api", 99);
    h.core.send(Intent::SetDraft(pr.clone(), true));
    until(&h.core, "under way", |s| {
        s.acting.contains(&crate::Acting::Draft(pr.clone()))
    })
    .await;
    let s = until(&h.core, "done", |s| s.acting.is_empty()).await;
    assert_eq!(
        s.flash.as_ref().map(|(_, text)| text.as_str()),
        Some("#99 is a draft again")
    );
}
