//! Issues and pull requests linked to a flow (CTX-2): the issue it was
//! started for, and those its agents link with Leon's `link_github` tool.
//! Each is read from GitHub for Info when linked, at startup and at each
//! tick, like the folders' pull requests.

use std::collections::HashSet;
use std::sync::Arc;

use leon_domain::{FlowId, GhKind, GhRef};

use crate::app::Core;
use crate::repo::Internal;

impl Core {
    /// Links `item` to flow `id`, once, and reads it.
    pub(crate) fn link(&mut self, id: FlowId, item: GhRef) {
        let new = self
            .state
            .flows
            .iter()
            .find(|c| c.id == id)
            .is_some_and(|c| !c.links.contains(&item));
        if new {
            self.edit(id, |c| c.links.push(item.clone()));
        }
        self.read_link(item);
    }

    /// Reads every link of the flows not archived.
    pub(crate) fn refresh_links(&self) {
        let mut seen = HashSet::new();
        let links: Vec<GhRef> = self
            .state
            .flows
            .iter()
            .filter(|c| c.archived.is_none())
            .flat_map(|c| c.links.iter().cloned())
            .filter(|item| seen.insert(item.clone()))
            .collect();
        for item in links {
            self.read_link(item);
        }
    }

    /// Whether `item` is linked to a flow: an action on it reads it again.
    pub(crate) fn is_linked(&self, item: &GhRef) -> bool {
        self.state.flows.iter().any(|c| c.links.contains(item))
    }

    /// Reads a linked issue or pull request in the background.
    pub(crate) fn read_link(&self, item: GhRef) {
        let github = Arc::clone(&self.ports.github);
        let tx = self.internal_tx.clone();
        crate::jobs::spawn(move || {
            let read = match item.kind {
                GhKind::Issue => github
                    .issue(&item.repo, item.number)
                    .map(|issue| Internal::LinkedIssue(item.clone(), Box::new(issue))),
                GhKind::PullRequest => github
                    .pull_request_by_number(&item.repo, item.number)
                    .map(|pr| Internal::LinkedPr(item.clone(), Box::new(pr))),
            };
            match read {
                Ok(found) => {
                    let _ = tx.send(found);
                }
                Err(error) => {
                    tracing::debug!(%error, item = %item.text(), "reading a linked item");
                }
            }
        });
    }
}
