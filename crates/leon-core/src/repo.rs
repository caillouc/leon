//! Git and GitHub state of conversations (GIT-*, GH-*, NAV-6, CONV-14).
//!
//! Every git or gh call runs on the blocking pool; its result comes back to
//! the core actor as an [`Internal`] message, so the actor never waits.

use std::collections::HashMap;
use std::sync::Arc;

use leon_domain::{ConversationId, GhKind, GhRef, Issue, PullRequest};

use crate::app::Core;
use crate::snapshot::{Detail, GitView};

/// Results of background work, applied by the core actor.
#[derive(Debug)]
pub(crate) enum Internal {
    /// Fresh git state of a conversation's working directory.
    Git(ConversationId, GitView),
    /// The pull request of a conversation's branch, or none.
    Pr(ConversationId, Option<Box<PullRequest>>),
    /// My issues and pull requests (GH-2, GH-3), or why they failed.
    Lists(Result<(Vec<Issue>, Vec<PullRequest>), String>),
    /// A GitHub action on an item finished: say so, and read the item again
    /// where it shows.
    Done(GhRef, String),
    /// An item's details, read for the drawer.
    Detail(Detail),
    /// Time to refresh pull request badges.
    Tick,
    /// A title the CLI wrote for a conversation (NAV-9), with the title it
    /// replaces.
    Title(ConversationId, String, String),
    /// The doctor's report.
    Doctor(Vec<leon_domain::DoctorSection>),
    /// What each CLI is connected to (TOOLS-1).
    Tools(leon_domain::CliConnectors),
}

impl Core {
    /// Re-reads git state; coalesces calls while one is running.
    pub(crate) fn refresh_git(&mut self, id: ConversationId) {
        if !self.git_in_flight.insert(id) {
            self.git_dirty.insert(id);
            return;
        }
        let Some(dir) = self.conversation_dir(id) else {
            self.git_in_flight.remove(&id);
            return;
        };
        let git = Arc::clone(&self.ports.git);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let branch = git.branch(&dir);
            let base = git.default_base(&dir);
            let changes = git.changes(&dir);
            let branch_diff = base
                .as_deref()
                .map(|b| git.branch_diff(&dir, b))
                .transpose();
            let error = changes.as_ref().err().map(ToString::to_string);
            let view = GitView {
                branch,
                base,
                changes: Arc::new(changes.unwrap_or_default()),
                branch_diff: Arc::new(branch_diff.ok().flatten().unwrap_or_default()),
                error,
            };
            let _ = tx.send(Internal::Git(id, view));
        });
    }

    /// Re-reads the pull request of a conversation's branch.
    pub(crate) fn refresh_pr(&mut self, id: ConversationId) {
        let (Some(dir), Some(branch)) = (
            self.conversation_dir(id),
            self.state.git.get(&id).and_then(|g| g.branch.clone()),
        ) else {
            return;
        };
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || match github.pull_request(&dir, &branch) {
            Ok(pr) => {
                let _ = tx.send(Internal::Pr(id, pr.map(Box::new)));
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
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let message = match github.request_copilot_review(&item.repo, item.number) {
                Ok(()) => format!("Copilot review requested on #{}", item.number),
                Err(error) => format!("Could not request a Copilot review: {error}"),
            };
            let _ = tx.send(Internal::Done(item, message));
        });
    }

    /// Posts a comment on a pull request or an issue (GH-6).
    pub(crate) fn comment(&mut self, item: GhRef, body: String) {
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let message = match github.comment(&item.repo, item.number, &body) {
                Ok(()) => format!("Comment posted on #{}", item.number),
                Err(error) => format!("Could not post the comment: {error}"),
            };
            let _ = tx.send(Internal::Done(item, message));
        });
    }

    /// Marks a pull request as a draft or ready for review (GH-7).
    pub(crate) fn set_draft(&mut self, item: GhRef, draft: bool) {
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let message = match (github.set_draft(&item.repo, item.number, draft), draft) {
                (Ok(()), true) => format!("#{} is a draft again", item.number),
                (Ok(()), false) => format!("#{} is ready for review", item.number),
                (Err(error), _) => format!("Could not change #{}: {error}", item.number),
            };
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
            Internal::Git(id, view) => {
                self.git_in_flight.remove(&id);
                let branch_changed =
                    self.state.git.get(&id).map(|g| &g.branch) != Some(&view.branch);
                Arc::make_mut(&mut self.state.git).insert(id, view);
                if self.git_dirty.remove(&id) {
                    self.refresh_git(id);
                }
                if branch_changed {
                    self.refresh_pr(id);
                }
            }
            Internal::Pr(id, pr) => {
                let prs = Arc::make_mut(&mut self.state.prs);
                match pr {
                    Some(pr) => prs.insert(id, *pr),
                    None => prs.remove(&id),
                };
            }
            Internal::Lists(Ok((issues, prs))) => {
                self.state.my_issues = Arc::new(issues);
                self.state.my_prs = Arc::new(prs);
                self.state.github_error = None;
            }
            Internal::Lists(Err(error)) => self.state.github_error = Some(error),
            Internal::Done(item, message) => {
                self.flash(message);
                let shown: Vec<ConversationId> = self
                    .state
                    .prs
                    .iter()
                    .filter(|(_, pr)| pr.repo == item.repo && pr.number == item.number)
                    .map(|(id, _)| *id)
                    .collect();
                for id in shown {
                    self.refresh_pr(id);
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
                let ids: Vec<ConversationId> = self
                    .state
                    .conversations
                    .iter()
                    .filter(|c| !c.archived)
                    .map(|c| c.id)
                    .collect();
                for id in ids {
                    self.refresh_pr(id);
                }
            }
        }
    }

    /// Reads git state of every open conversation (startup).
    pub(crate) fn refresh_all_git(&mut self) {
        let ids: Vec<ConversationId> = self
            .state
            .conversations
            .iter()
            .filter(|c| !c.archived)
            .map(|c| c.id)
            .collect();
        for id in ids {
            self.refresh_git(id);
        }
    }
}

/// Git state kept per conversation.
pub(crate) type GitMap = Arc<HashMap<ConversationId, GitView>>;
