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
use crate::snapshot::{Acting, Detail, GitView};

/// How long a fetched base stays fresh: no new fetch before (I22).
const FETCH_EVERY_MS: i64 = 5 * 60 * 1000;

/// Results of background work, applied by the core actor.
#[derive(Debug)]
pub(crate) enum Internal {
    /// Fresh git state of a folder.
    Git(PathBuf, GitView),
    /// A folder's base branch was fetched: its branch diff is read again.
    Fetched(PathBuf),
    /// The pull request of a folder's branch, or none.
    Pr(PathBuf, Option<Box<PullRequest>>),
    /// The working tree holding a folder of a flow, `None` outside a
    /// repository (CTX-2). `keep`: the folder joins the flow even
    /// then (an agent said it works there).
    WorkingTree {
        conversation: leon_domain::ConversationId,
        /// Which of its conversation's folders this is, in order.
        seq: u64,
        flow: FlowId,
        dir: PathBuf,
        root: Option<PathBuf>,
        keep: bool,
    },
    /// My issues and pull requests (GH-2, GH-3), or why they failed.
    Lists(Result<(Vec<Issue>, Vec<PullRequest>), String>),
    /// A GitHub action on an item finished: say so, and read the item again
    /// where it shows.
    Done(GhRef, Acting, String),
    /// An item's details, read for the drawer.
    Detail(Detail),
    /// A pull request's changes, read from GitHub (GH-3).
    PrDiff(GhRef, Result<Arc<Vec<leon_domain::FileDiff>>, String>),
    /// A linked issue, read (CTX-2).
    LinkedIssue(GhRef, Box<Issue>),
    /// A linked pull request, read (CTX-2).
    LinkedPr(GhRef, Box<PullRequest>),
    /// Time to refresh pull request badges.
    Tick,
    /// Between two ticks: time to refresh the flow looked at, if its checks
    /// still run.
    QuickTick,
    /// A title the CLI wrote for a flow (NAV-9), with the title it
    /// replaces.
    Title(FlowId, String, String),
    /// The doctor's report.
    Doctor(Vec<leon_domain::DoctorSection>),
    /// What each CLI is connected to (TOOLS-1).
    Tools(leon_domain::CliConnectors),
    /// Leon's latest release, or why it could not be read (DOCTOR-3).
    Release(Result<leon_domain::Release, String>),
    /// The newer Leon is in place, or why not (DOCTOR-3).
    Installed(Result<(), String>),
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
        crate::jobs::spawn(move || {
            let branch = git.branch(&dir);
            let main = git.is_main_checkout(&dir);
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
                main,
                base,
                changes: Arc::new(changes.unwrap_or_default()),
                branch_diff: Arc::new(branch_diff.ok().flatten().unwrap_or_default()),
                error,
            };
            let _ = tx.send(Internal::Git(dir, view));
        });
    }

    /// Brings the base of each folder's branch diff up to date from the
    /// remote, in the background and at most every five minutes, then reads
    /// the diff again (GIT-3, I22). A failure (offline, no access) leaves
    /// the diff as it was.
    pub(crate) fn fetch_bases(&mut self, id: FlowId) {
        let now = self.now_ms();
        for dir in self.state.folders_of(id) {
            let Some(branch) = self.remote_base(&dir) else {
                continue;
            };
            let key = (dir.clone(), branch.clone());
            if self
                .fetched
                .get(&key)
                .is_some_and(|at| now - at < FETCH_EVERY_MS)
            {
                continue;
            }
            self.fetched.insert(key, now);
            let git = Arc::clone(&self.ports.git);
            let tx = self.internal_tx.clone();
            crate::jobs::spawn(move || {
                if git.fetch(&dir, &branch).is_ok() {
                    let _ = tx.send(Internal::Fetched(dir));
                } else {
                    // git's message may hold the remote's URL: not logged.
                    tracing::debug!(dir = %dir.display(), "fetching the base failed");
                }
            });
        }
    }

    /// The remote branch `dir`'s branch diff is compared with, without
    /// `origin/`: its pull request's base, else the base read last.
    fn remote_base(&self, dir: &std::path::Path) -> Option<String> {
        let base = self
            .pr_base(dir)
            .or_else(|| self.state.git.get(dir)?.base.clone())?;
        base.strip_prefix("origin/").map(str::to_owned)
    }

    /// The branch the pull request of `dir`'s branch merges into, on the
    /// remote: the branch diff's base (GIT-3).
    pub(crate) fn pr_base(&self, dir: &std::path::Path) -> Option<String> {
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
        crate::jobs::spawn(move || match github.pull_request(&dir, &branch) {
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
        crate::jobs::spawn(move || {
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
            Acting::CopilotReview(item.clone()),
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
        let acting = Acting::Comment(item.clone());
        self.act(
            acting,
            item,
            "Could not post the comment",
            move |github, item| {
                github
                    .comment(&item.repo, item.number, &body)
                    .map(|()| format!("Comment posted on #{}", item.number))
            },
        );
    }

    /// Marks a pull request as a draft or ready for review (GH-7).
    pub(crate) fn set_draft(&mut self, item: GhRef, draft: bool) {
        self.act(
            Acting::Draft(item.clone()),
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

    /// Answers a review thread of pull request `pr`, then resolves it when
    /// `resolve`; a failed answer resolves nothing (GH-8).
    pub(crate) fn reply_to_thread(
        &mut self,
        pr: GhRef,
        thread: String,
        body: String,
        resolve: bool,
    ) {
        let acting = Acting::Reply(thread.clone());
        self.act(
            acting,
            pr,
            "Could not answer the thread",
            move |github, _| {
                github.reply_to_thread(&thread, &body)?;
                if !resolve {
                    return Ok("Answer posted".to_owned());
                }
                github
                    .resolve_thread(&thread)
                    .map(|()| "Answered and resolved".to_owned())
            },
        );
    }

    /// Resolves a review thread of pull request `pr` as fixed: answers
    /// "Fixed", then resolves it; a failed answer resolves nothing (GH-8).
    pub(crate) fn resolve_thread(&mut self, pr: GhRef, thread: String) {
        let acting = Acting::Resolve(thread.clone());
        self.act(
            acting,
            pr,
            "Could not resolve the thread",
            move |github, _| {
                github.reply_to_thread(&thread, "Fixed")?;
                github
                    .resolve_thread(&thread)
                    .map(|()| "Resolved as fixed".to_owned())
            },
        );
    }

    /// Runs a GitHub action on `item` in the background — its button shows
    /// it under way (`acting`) at once — then says how it went (`run`'s
    /// message, or `failed` and why) and reads the item again where it
    /// shows.
    fn act(
        &mut self,
        acting: Acting,
        item: GhRef,
        failed: &'static str,
        run: impl FnOnce(&dyn GitHub, &GhRef) -> Result<String, PortError> + Send + 'static,
    ) {
        Arc::make_mut(&mut self.state.acting).insert(acting.clone());
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        crate::jobs::spawn(move || {
            let message =
                run(github.as_ref(), &item).unwrap_or_else(|error| format!("{failed}: {error}"));
            let _ = tx.send(Internal::Done(item, acting, message));
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

    /// Reads a pull request's changes from GitHub, as its "Files changed"
    /// shows them (GH-3); `None` closes them.
    pub(crate) fn show_pr_diff(&mut self, item: Option<GhRef>) {
        let Some(item) = item else {
            self.state.pr_diff = None;
            return;
        };
        self.state.pr_diff = Some(crate::PrDiff {
            pr: item.clone(),
            files: None,
        });
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        crate::jobs::spawn(move || {
            let files = github
                .pull_request_diff(&item.repo, item.number)
                .map(Arc::new)
                .map_err(|e| e.to_string());
            let _ = tx.send(Internal::PrDiff(item, files));
        });
    }

    fn load_detail(&self, item: GhRef) {
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        crate::jobs::spawn(move || {
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
            Internal::Fetched(dir) => self.refresh_folder(dir),
            Internal::Pr(dir, pr) => self.folder_pr_read(dir, pr.map(|pr| *pr)),
            Internal::WorkingTree {
                conversation,
                seq,
                flow,
                dir,
                root,
                keep,
            } => self.working_tree_found((conversation, seq), flow, dir, root, keep),
            Internal::Lists(Ok((issues, prs))) => self.lists_read(issues, prs),
            Internal::Lists(Err(error)) => self.state.github_error = Some(error),
            Internal::Done(item, acting, message) => {
                Arc::make_mut(&mut self.state.acting).remove(&acting);
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
                if self.is_linked(&item) {
                    self.read_link(item.clone());
                }
                if self.state.detail.as_ref().map(Detail::item) == Some(item.clone()) {
                    self.load_detail(item);
                }
            }
            Internal::Detail(detail) => {
                // Only the item still shown, not one closed or replaced since.
                if self.state.detail.as_ref().map(Detail::item) == Some(detail.item()) {
                    if let Detail::PullRequest(pr) = &detail {
                        self.fresh_pr(pr);
                    }
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
            Internal::Release(release) => self.release_read(release),
            Internal::Installed(result) => self.update_installed(result),
            Internal::PrDiff(item, files) => {
                // Only the pull request still shown, not one closed since.
                if let Some(diff) = self.state.pr_diff.as_mut().filter(|d| d.pr == item) {
                    diff.files = Some(files);
                }
            }
            Internal::LinkedIssue(item, issue) => {
                Arc::make_mut(&mut self.state.linked_issues).insert(item, *issue);
            }
            Internal::LinkedPr(item, pr) => {
                self.fresh_pr(&pr);
                Arc::make_mut(&mut self.state.linked_prs).insert(item, *pr);
            }
            Internal::QuickTick => self.quick_refresh(),
            Internal::Tick => {
                self.stop_idle_clis();
                self.purge_archive();
                self.check_update(false);
                for dir in self.open_folders() {
                    self.refresh_folder_pr(dir);
                }
                self.refresh_links();
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
            .filter(|c| c.archived.is_none())
            .flat_map(|c| self.state.folders_of(c.id))
            .filter(|dir| seen.insert(dir.clone()))
            .collect()
    }
}

/// Git state kept per folder.
pub(crate) type GitMap = Arc<HashMap<PathBuf, GitView>>;
