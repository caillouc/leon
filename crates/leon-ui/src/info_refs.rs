//! The Info tab (CTX-1, CTX-2): the whole flow's pull requests and issues,
//! whatever their repository or worktree, as every list shows them
//! (design.md, "PR and issue row"): pull requests by state, a merged or
//! closed one folded to one line; then the issues.

use gpui::{AnyElement, Context, IntoElement, div, prelude::*};
use leon_core::FlowView;

use crate::gh_rows::{Fold, issue_ref_row, pr_row};
use crate::widgets::label;
use crate::workspace::Workspace;

impl Workspace {
    /// The flow's pull requests, by state (open, draft, merged, closed),
    /// then its issues, each group under its label; `None` without any.
    pub(crate) fn info_refs(&self, view: &FlowView, cx: &mut Context<Self>) -> Option<AnyElement> {
        let prs = self.snapshot.prs_of(view);
        let issues: Vec<AnyElement> = view
            .issues
            .iter()
            .map(|item| match self.known_issue(item) {
                Some(issue) => self.issue_row(issue, false, cx),
                None => issue_ref_row(item),
            })
            .collect();
        if prs.is_empty() && issues.is_empty() {
            return None;
        }
        Some(
            div()
                .px_1()
                .flex()
                .flex_col()
                .when(!prs.is_empty(), |d| {
                    d.child(label("Pull requests"))
                        .children(prs.into_iter().map(|pr| pr_row(pr, Fold::Done, None)))
                })
                .when(!issues.is_empty(), |d| {
                    d.child(label("Issues")).children(issues)
                })
                .into_any_element(),
        )
    }
}
