//! The folders a flow works in (CTX-2, D3): every working tree —
//! a repository or one of its worktrees — its agents change files in or say
//! they moved to, in the order Leon saw them. A worktree an agent is merely
//! started in joins too; a repository's own checkout does not: it is where
//! the user keeps the repository, often on another flow's branch.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use leon_domain::{ConversationId, FlowId};

use crate::app::Core;
use crate::repo::Internal;

/// How a conversation came to a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Came {
    /// It changed a file there.
    Changed,
    /// It said it works there (`set_working_directory`).
    Said,
    /// It was started there by another agent.
    Started,
}

impl Core {
    /// Files an conversation changed: the working trees holding them join
    /// its flow.
    pub(crate) fn files_changed(&mut self, conversation: ConversationId, paths: &[PathBuf]) {
        let Some(s) = self.state.conversations.get(&conversation) else {
            return;
        };
        let flow = s.conversation.flow;
        let mut dirs: Vec<PathBuf> = paths
            .iter()
            .filter_map(|p| {
                normalize(&s.conversation.working_dir.join(p))
                    .parent()
                    .map(Path::to_path_buf)
            })
            .collect();
        dirs.sort();
        dirs.dedup();
        for dir in dirs {
            match self.working_trees.get(&dir).cloned() {
                Some(root) => {
                    let seq = self.next_lookup(conversation);
                    self.worked_in(conversation, flow, root, seq);
                }
                None => self.find_working_tree(conversation, flow, dir, Came::Changed),
            }
        }
    }

    /// Looks for the working tree holding `dir`, where `conversation`
    /// `came`, in the background, and whether it is a repository's own
    /// checkout.
    pub(crate) fn find_working_tree(
        &mut self,
        conversation: ConversationId,
        flow: FlowId,
        dir: PathBuf,
        came: Came,
    ) {
        let seq = self.next_lookup(conversation);
        let git = Arc::clone(&self.ports.git);
        let tx = self.internal_tx.clone();
        crate::jobs::spawn(move || {
            let root = git.working_tree(&dir);
            let main = root.as_deref().is_some_and(|r| git.is_main_checkout(r));
            let _ = tx.send(Internal::WorkingTree {
                conversation,
                seq,
                flow,
                dir,
                root,
                main,
                came,
            });
        });
    }

    /// Numbers a folder `conversation` works in, in the order it happened.
    fn next_lookup(&mut self, conversation: ConversationId) -> u64 {
        self.lookups += 1;
        self.latest_lookup.insert(conversation, self.lookups);
        self.lookups
    }

    /// The working tree holding `dir` (`main`: a repository's own
    /// checkout), where `conversation` came: it joins the flow, unless the
    /// conversation was only started in a repository's own checkout.
    pub(crate) fn working_tree_found(
        &mut self,
        (conversation, seq): (ConversationId, u64),
        flow: FlowId,
        dir: PathBuf,
        (root, main): (Option<PathBuf>, bool),
        came: Came,
    ) {
        match root {
            Some(root) => {
                self.working_trees.insert(dir, root.clone());
                if !(main && came == Came::Started) {
                    self.worked_in(conversation, flow, root, seq);
                }
            }
            // Outside a repository: only where it said or was started.
            None if came != Came::Changed => self.worked_in(conversation, flow, dir, seq),
            None => {}
        }
    }

    /// `conversation` works in `folder` (its `seq`th folder): the folder
    /// joins its flow, and is where the right pane shows it working unless
    /// it has worked somewhere since.
    fn worked_in(&mut self, conversation: ConversationId, flow: FlowId, folder: PathBuf, seq: u64) {
        let latest = self.latest_lookup.get(&conversation) == Some(&seq);
        let moved = self
            .state
            .conversations
            .get_mut(&conversation)
            .filter(|s| latest && s.conversation.folder.as_ref() != Some(&folder))
            .map(|s| s.conversation.folder = Some(folder.clone()))
            .is_some();
        if moved {
            self.save_conversation(conversation);
        }
        self.add_folder(flow, folder);
    }

    /// Adds a folder to a flow, once, and reads its git state (its
    /// pull request follows).
    fn add_folder(&mut self, flow: FlowId, folder: PathBuf) {
        let new = self
            .state
            .flows
            .iter()
            .find(|c| c.id == flow)
            .is_some_and(|c| !c.folders.contains(&folder));
        if new {
            self.edit(flow, |c| c.folders.push(folder.clone()));
            self.refresh_folder(folder);
        }
    }
}

/// `path` with its `.` and `..` parts resolved without reading the disk: a
/// CLI may give `../other-repo/file`.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}
