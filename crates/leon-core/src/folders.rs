//! The folders a flow works in (CTX-2, D3): every working tree —
//! a repository or one of its worktrees — its agents change files in or say
//! they moved to, in the order Leon saw them.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use leon_domain::{ConversationId, FlowId};

use crate::app::Core;
use crate::repo::Internal;

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
            match self.working_trees.get(&dir) {
                Some(root) => self.add_folder(flow, root.clone()),
                None => self.find_working_tree(flow, dir, false),
            }
        }
    }

    /// Looks for the working tree holding `dir` in the background. `keep`:
    /// `dir` joins the flow even outside a repository.
    pub(crate) fn find_working_tree(&self, flow: FlowId, dir: PathBuf, keep: bool) {
        let git = Arc::clone(&self.ports.git);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let root = git.working_tree(&dir);
            let _ = tx.send(Internal::WorkingTree {
                flow,
                dir,
                root,
                keep,
            });
        });
    }

    pub(crate) fn working_tree_found(
        &mut self,
        flow: FlowId,
        dir: PathBuf,
        root: Option<PathBuf>,
        keep: bool,
    ) {
        match root {
            Some(root) => {
                self.working_trees.insert(dir, root.clone());
                self.add_folder(flow, root);
            }
            None if keep => self.add_folder(flow, dir),
            None => {}
        }
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
