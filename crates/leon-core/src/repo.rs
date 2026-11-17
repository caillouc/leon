//! Git and GitHub state of conversations (GIT-*, GH-*, NAV-6, CONV-14).
//!
//! Every git or gh call runs on the blocking pool; its result comes back to
//! the core actor as an [`Internal`] message, so the actor never waits.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_domain::{ConversationId, Issue, PullRequest};

use crate::app::Core;
use crate::snapshot::GitView;

/// Results of background work, applied by the core actor.
#[derive(Debug)]
pub(crate) enum Internal {
    /// Fresh git state of a conversation's working directory.
    Git(ConversationId, GitView),
    /// The pull request of a conversation's branch, or none.
    Pr(ConversationId, Option<PullRequest>),
    /// My issues and pull requests (GH-2, GH-3), or why they failed.
    Lists(Result<(Vec<Issue>, Vec<PullRequest>), String>),
    /// A GitHub action finished: refresh this conversation's PR and say so.
    Done(ConversationId, String),
    /// Time to refresh pull request badges.
    Tick,
}

impl Core {
    /// Working directory of the session the user started in a conversation.
    fn root_dir(&self, id: ConversationId) -> Option<PathBuf> {
        self.state
            .sessions
            .values()
            .filter(|s| s.session.conversation == id)
            .find(|s| s.session.started_by.is_none())
            .map(|s| s.session.working_dir.clone())
    }

    /// Re-reads git state; coalesces calls while one is running.
    pub(crate) fn refresh_git(&mut self, id: ConversationId) {
        if !self.git_in_flight.insert(id) {
            self.git_dirty.insert(id);
            return;
        }
        let Some(dir) = self.root_dir(id) else {
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
            self.root_dir(id),
            self.state.git.get(&id).and_then(|g| g.branch.clone()),
        ) else {
            return;
        };
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || match github.pull_request(&dir, &branch) {
            Ok(pr) => {
                let _ = tx.send(Internal::Pr(id, pr));
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

    /// Asks Copilot to review the conversation's pull request (GH-5).
    pub(crate) fn request_review(&mut self, id: ConversationId) {
        let Some(pr) = self.state.prs.get(&id).cloned() else {
            return;
        };
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let message = match github.request_copilot_review(&pr.repo, pr.number) {
                Ok(()) => format!("Copilot review requested on #{}", pr.number),
                Err(error) => format!("Could not request a Copilot review: {error}"),
            };
            let _ = tx.send(Internal::Done(id, message));
        });
    }

    /// Posts a comment on the conversation's pull request (GH-6).
    pub(crate) fn comment(&mut self, id: ConversationId, body: String) {
        let Some(pr) = self.state.prs.get(&id).cloned() else {
            return;
        };
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let message = match github.comment(&pr.repo, pr.number, &body) {
                Ok(()) => format!("Comment posted on #{}", pr.number),
                Err(error) => format!("Could not post the comment: {error}"),
            };
            let _ = tx.send(Internal::Done(id, message));
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
                    Some(pr) => prs.insert(id, pr),
                    None => prs.remove(&id),
                };
            }
            Internal::Lists(Ok((issues, prs))) => {
                self.state.my_issues = Arc::new(issues);
                self.state.my_prs = Arc::new(prs);
                self.state.github_error = None;
            }
            Internal::Lists(Err(error)) => self.state.github_error = Some(error),
            Internal::Done(id, message) => {
                self.flash(message);
                self.refresh_pr(id);
            }
            Internal::Tick => {
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
