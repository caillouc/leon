//! Keeping what Leon shows of GitHub up to date (CTX-1, GH-3): a pull
//! request read anywhere (its details, a folder's branch, a link, my list)
//! replaces every copy Leon keeps of it, and the flow looked at is read
//! again often while its checks run.

use std::path::PathBuf;
use std::sync::Arc;

use leon_domain::{CheckState, FlowId, GhRef, Issue, PullRequest};

use crate::app::Core;
use crate::snapshot::Detail;

impl Core {
    /// `pr` was just read from GitHub: every copy of it takes it, so the
    /// flow's Info, the lists and the details never disagree.
    pub(crate) fn fresh_pr(&mut self, pr: &PullRequest) {
        let same = |p: &PullRequest| p.repo == pr.repo && p.number == pr.number && p != pr;
        if self.state.prs.values().any(same) {
            for p in Arc::make_mut(&mut self.state.prs).values_mut() {
                if same(p) {
                    p.clone_from(pr);
                }
            }
        }
        let item = GhRef::pull_request(&pr.repo, pr.number);
        if self.state.linked_prs.get(&item).is_some_and(same) {
            Arc::make_mut(&mut self.state.linked_prs).insert(item, pr.clone());
        }
        if self.state.my_prs.iter().any(same) {
            for p in Arc::make_mut(&mut self.state.my_prs) {
                if same(p) {
                    p.clone_from(pr);
                }
            }
        }
        if let Some(Detail::PullRequest(shown)) = &mut self.state.detail
            && same(shown)
        {
            **shown = pr.clone();
        }
    }

    /// The pull request of folder `dir`'s branch, or none, just read.
    pub(crate) fn folder_pr_read(&mut self, dir: PathBuf, pr: Option<PullRequest>) {
        let before = self.pr_base(&dir);
        match pr {
            Some(pr) => {
                self.fresh_pr(&pr);
                Arc::make_mut(&mut self.state.prs).insert(dir.clone(), pr);
            }
            None => {
                Arc::make_mut(&mut self.state.prs).remove(&dir);
            }
        }
        // Another base: the branch diff is compared again (GIT-3).
        if self.pr_base(&dir) != before {
            self.refresh_folder(dir);
        }
    }

    /// My issues and pull requests, just read (GH-2, GH-3).
    pub(crate) fn lists_read(&mut self, issues: Vec<Issue>, prs: Vec<PullRequest>) {
        self.state.my_issues = Arc::new(issues);
        for pr in &prs {
            self.fresh_pr(pr);
        }
        self.state.my_prs = Arc::new(prs);
        self.state.github_error = None;
    }

    /// Reads again the pull requests and links of flow `id`.
    pub(crate) fn refresh_flow_github(&mut self, id: FlowId) {
        self.refresh_pr(id);
        for item in self.links_of(id) {
            self.read_link(item);
        }
    }

    /// The issues and pull requests linked to flow `id`.
    fn links_of(&self, id: FlowId) -> Vec<GhRef> {
        self.state
            .flows
            .iter()
            .find(|c| c.id == id)
            .map_or_default(|c| c.links.clone())
    }

    /// Between two full refreshes: the flow looked at is read again while
    /// one of its pull requests' checks still runs, so its count moves.
    pub(crate) fn quick_refresh(&mut self) {
        let Some(id) = self.viewing else { return };
        let links = self.links_of(id);
        let running = self
            .state
            .folders_of(id)
            .iter()
            .filter_map(|f| self.state.prs.get(f))
            .chain(links.iter().filter_map(|l| self.state.linked_prs.get(l)))
            .any(|pr| pr.checks_state() == Some(CheckState::Pending));
        if running {
            self.refresh_flow_github(id);
        }
    }
}
