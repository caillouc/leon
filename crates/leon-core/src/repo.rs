//! Git and GitHub state of flows (GIT-*, GH-*, NAV-6, CONV-14).
//!
//! Every git or gh call runs on the blocking pool; its result comes back to
//! the core actor as an [`Internal`] message, so the actor never waits.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use leon_domain::{FlowId, GhKind, GhRef, Issue, PullRequest};

use crate::app::Core;
use crate::ports::{GitHub, PortError};
use crate::snapshot::{Detail, GitView};

/// Results of background work, applied by the core actor.
#[derive(Debug)]
pub(crate) enum Internal {
    /// Fresh git state of a folder.
    Git(PathBuf, GitView),
    /// The pull request of a folder's branch, or none.
    Pr(PathBuf, Option<Box<PullRequest>>),
    /// The working tree holding a folder of a flow, `None` outside a
    /// repository (CTX-2). `keep`: the folder joins the flow even
    /// then (an agent said it works there).
    WorkingTree {
        flow: FlowId,
        dir: PathBuf,
        root: Option<PathBuf>,
        keep: bool,
    },
    /// My issues and pull requests (GH-2, GH-3), or why they failed.
    Lists(Result<(Vec<Issue>, Vec<PullRequest>), String>),
    /// A GitHub action on an item finished: say so, and read the item again
    /// where it shows.
    Done(GhRef, String),
    /// An item's details, read for the drawer.
    Detail(Detail),
    /// Time to refresh pull request badges.
    Tick,
    /// A title the CLI wrote for a flow (NAV-9), with the title it
    /// replaces.
    Title(FlowId, String, String),
    /// The doctor's report.
    Doctor(Vec<leon_domain::DoctorSection>),
    /// What each CLI is connected to (TOOLS-1).
    Tools(leon_domain::CliConnectors),
}

impl Core {
    /// Re-reads the git state of every folder of a flow.
    pub(crate) fn refresh_git(&mut self, id: FlowId) {
        for dir in self.state.folders_of(id) {
            self.refresh_folder(dir);
        }
    }

    /// Re-reads a folder's git state; coalesces calls while one is running.
    pub(crate) fn refresh_folder(&mut self, dir: PathBuf) {
        if !self.git_in_flight.insert(dir.clone()) {
            self.git_dirty.insert(dir);
            return;
        }
        let git = Arc::clone(&self.ports.git);
        let tx = self.internal_tx.clone();
        let pr_base = self.pr_base(&dir);
        tokio::task::spawn_blocking(move || {
            let branch = git.branch(&dir);
            // What its pull request merges into, as GitHub shows the diff
            // (GIT-3); without one, or when that branch is not fetched, the
            // remote's default branch.
            let from_pr = pr_base.and_then(|b| git.branch_diff(&dir, &b).ok().map(|d| (b, d)));
            let (base, branch_diff) = if let Some((base, diff)) = from_pr {
                (Some(base), Ok(Some(diff)))
            } else {
                let base = git.default_base(&dir);
                let diff = base
                    .as_deref()
                    .map(|b| git.branch_diff(&dir, b))
                    .transpose();
                (base, diff)
            };
            let changes = git.changes(&dir);
            let error = changes.as_ref().err().map(ToString::to_string);
            let view = GitView {
                repo: git.github_repo(&dir),
                branch,
                base,
                changes: Arc::new(changes.unwrap_or_default()),
                branch_diff: Arc::new(branch_diff.ok().flatten().unwrap_or_default()),
                error,
            };
            let _ = tx.send(Internal::Git(dir, view));
        });
    }

    /// The branch the pull request of `dir`'s branch merges into, on the
    /// remote: the branch diff's base (GIT-3).
    fn pr_base(&self, dir: &std::path::Path) -> Option<String> {
        self.state
            .prs
            .get(dir)
            .map(|pr| format!("origin/{}", pr.base))
    }

    /// Re-reads the pull request of each folder's branch of a flow.
    pub(crate) fn refresh_pr(&mut self, id: FlowId) {
        for dir in self.state.folders_of(id) {
            self.refresh_folder_pr(dir);
        }
    }

    /// Re-reads the pull request of a folder's branch.
    fn refresh_folder_pr(&self, dir: PathBuf) {
        let Some(branch) = self.state.git.get(&dir).and_then(|g| g.branch.clone()) else {
            return;
        };
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || match github.pull_request(&dir, &branch) {
            Ok(pr) => {
                let _ = tx.send(Internal::Pr(dir, pr.map(Box::new)));
            }
            Err(error) => tracing::debug!(%error, "pull request refresh"),
        });
    }

    /// Re-reads my issues and pull requests.
    pub(crate) fn refresh_lists(&mut self) {
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let lists = github
                .my_issues()
                .and_then(|issues| github.my_pull_requests().map(|prs| (issues, prs)))
                .map_err(|e| e.to_string());
            let _ = tx.send(Internal::Lists(lists));
        });
    }

    /// Asks Copilot to review a pull request (GH-5).
    pub(crate) fn request_review(&mut self, item: GhRef) {
        self.act(
            item,
            "Could not request a Copilot review",
            |github, item| {
                github
                    .request_copilot_review(&item.repo, item.number)
                    .map(|()| format!("Copilot review requested on #{}", item.number))
            },
        );
    }

    /// Posts a comment on a pull request or an issue (GH-6).
    pub(crate) fn comment(&mut self, item: GhRef, body: String) {
        self.act(item, "Could not post the comment", move |github, item| {
            github
                .comment(&item.repo, item.number, &body)
                .map(|()| format!("Comment posted on #{}", item.number))
        });
    }

    /// Marks a pull request as a draft or ready for review (GH-7).
    pub(crate) fn set_draft(&mut self, item: GhRef, draft: bool) {
        self.act(
            item,
            "Could not change the pull request",
            move |github, item| {
                github.set_draft(&item.repo, item.number, draft).map(|()| {
                    let now = if draft {
                        "a draft again"
                    } else {
                        "ready for review"
                    };
                    format!("#{} is {now}", item.number)
                })
            },
        );
    }

    /// Answers a review thread of pull request `pr` (GH-8).
    pub(crate) fn reply_to_thread(&mut self, pr: GhRef, thread: String, body: String) {
        self.act(pr, "Could not answer the thread", move |github, _| {
            github
                .reply_to_thread(&thread, &body)
                .map(|()| "Answer posted".to_owned())
        });
    }

    /// Resolves a review thread of pull request `pr` (GH-8).
    pub(crate) fn resolve_thread(&mut self, pr: GhRef, thread: String) {
        self.act(pr, "Could not resolve the thread", move |github, _| {
            github
                .resolve_thread(&thread)
                .map(|()| "Thread resolved".to_owned())
        });
    }

    /// Runs a GitHub action on `item` in the background, then says how it
    /// went (`run`'s message, or `failed` and why) and reads the item again
    /// where it shows.
    fn act(
        &self,
        item: GhRef,
        failed: &'static str,
        run: impl FnOnce(&dyn GitHub, &GhRef) -> Result<String, PortError> + Send + 'static,
    ) {
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let message =
                run(github.as_ref(), &item).unwrap_or_else(|error| format!("{failed}: {error}"));
            let _ = tx.send(Internal::Done(item, message));
        });
    }

    /// Shows an item's details, read in the background; `None` closes them.
    pub(crate) fn show_detail(&mut self, item: Option<GhRef>) {
        let Some(item) = item else {
            self.state.detail = None;
            return;
        };
        self.state.detail = Some(Detail::Loading(item.clone()));
        self.load_detail(item);
    }

    fn load_detail(&self, item: GhRef) {
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let detail = match item.kind {
                GhKind::Issue => github
                    .issue(&item.repo, item.number)
                    .map(|i| Detail::Issue(Box::new(i))),
                GhKind::PullRequest => github
                    .pull_request_by_number(&item.repo, item.number)
                    .map(|pr| Detail::PullRequest(Box::new(pr))),
            };
            let detail = detail.unwrap_or_else(|e| Detail::Failed(item, e.to_string()));
            let _ = tx.send(Internal::Detail(detail));
        });
    }

    pub(crate) fn on_internal(&mut self, message: Internal) {
        match message {
            Internal::Git(dir, view) => {
                self.git_in_flight.remove(&dir);
                let branch_changed =
                    self.state.git.get(&dir).map(|g| &g.branch) != Some(&view.branch);
                Arc::make_mut(&mut self.state.git).insert(dir.clone(), view);
                if self.git_dirty.remove(&dir) {
                    self.refresh_folder(dir.clone());
                }
                if branch_changed {
                    self.refresh_folder_pr(dir);
                }
            }
            Internal::Pr(dir, pr) => {
                let before = self.pr_base(&dir);
                let prs = Arc::make_mut(&mut self.state.prs);
                match pr {
                    Some(pr) => prs.insert(dir.clone(), *pr),
                    None => prs.remove(&dir),
                };
                // Another base: the branch diff is compared again (GIT-3).
                if self.pr_base(&dir) != before {
                    self.refresh_folder(dir);
                }
            }
            Internal::WorkingTree {
                flow,
                dir,
                root,
                keep,
            } => self.working_tree_found(flow, dir, root, keep),
            Internal::Lists(Ok((issues, prs))) => {
                self.state.my_issues = Arc::new(issues);
                self.state.my_prs = Arc::new(prs);
                self.state.github_error = None;
            }
            Internal::Lists(Err(error)) => self.state.github_error = Some(error),
            Internal::Done(item, message) => {
                self.flash(message);
                let shown: Vec<PathBuf> = self
                    .state
                    .prs
                    .iter()
                    .filter(|(_, pr)| pr.repo == item.repo && pr.number == item.number)
                    .map(|(dir, _)| dir.clone())
                    .collect();
                for dir in shown {
                    self.refresh_folder_pr(dir);
                }
                if self.state.detail.as_ref().map(Detail::item) == Some(item.clone()) {
                    self.load_detail(item);
                }
            }
            Internal::Detail(detail) => {
                // Only the item still shown, not one closed or replaced since.
                if self.state.detail.as_ref().map(Detail::item) == Some(detail.item()) {
                    self.state.detail = Some(detail);
                }
            }
            Internal::Title(id, old, new) => {
                // A title the user or an agent already changed stays.
                self.edit(id, |c| {
                    if c.title == old {
                        c.title = new;
                    }
                });
            }
            Internal::Doctor(sections) => self.doctor_done(sections),
            Internal::Tools(cli) => self.tools_done(cli),
            Internal::Tick => {
                self.stop_idle_clis();
                for dir in self.open_folders() {
                    self.refresh_folder_pr(dir);
                }
            }
        }
    }

    /// Reads git state of every open flow's folders (startup).
    pub(crate) fn refresh_all_git(&mut self) {
        for dir in self.open_folders() {
            self.refresh_folder(dir);
        }
    }

    /// The folders of every flow not archived, each once.
    fn open_folders(&self) -> Vec<PathBuf> {
        let mut seen = HashSet::new();
        self.state
            .flows
            .iter()
            .filter(|c| !c.archived)
            .flat_map(|c| self.state.folders_of(c.id))
            .filter(|dir| seen.insert(dir.clone()))
            .collect()
    }
}

/// Git state kept per folder.
pub(crate) type GitMap = Arc<HashMap<PathBuf, GitView>>;
