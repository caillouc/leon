//! A flow's pull requests in its left-pane row (NAV-6): their icon and how
//! many, and on hover the list of them, one line each (design.md, "PR and
//! issue row", folded), opening Leon's view of it.

use gpui::{AnyElement, Context, IntoElement, Render, SharedString, Window, div, prelude::*};
use leon_domain::{FlowId, PrState, PullRequest};

use crate::github::pr_icon;
use crate::pr_view::pr_state;
use crate::theme;

/// The state shown for several: the most active (open, then draft, merged,
/// closed).
fn lead(prs: &[PullRequest]) -> PrState {
    prs.iter().map(|p| p.state).min().unwrap_or(PrState::Closed)
}

/// The icon in the leading state's colour, how many, and a red mark when
/// one of them is blocked (a failed check, a conflict); `None` without
/// pull requests.
pub(crate) fn flow_prs(id: FlowId, prs: Vec<PullRequest>) -> Option<AnyElement> {
    if prs.is_empty() {
        return None;
    }
    let state = lead(&prs);
    let blocked = prs.iter().any(crate::pr_view::blocked);
    Some(
        div()
            .id(SharedString::from(format!("flow-prs-{}", id.0)))
            .flex()
            .flex_none()
            .items_center()
            .gap(crate::theme::rem(3.))
            .px(crate::theme::rem(4.))
            .rounded(crate::theme::rem(4.))
            .text_color(pr_state(state).0)
            .hover(|s| s.bg(theme::surface_2()))
            .child(pr_icon(state, 13.))
            .child(prs.len().to_string())
            .when(blocked, |d| d.child(crate::widgets::mark(theme::err(), 6.)))
            .hoverable_tooltip(move |_, cx| cx.new(|_| PrList(prs.clone())).into())
            .into_any_element(),
    )
}

/// The list shown on hover.
struct PrList(Vec<PullRequest>);

impl Render for PrList {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(crate::theme::rem(340.))
            // Drawn outside the window's root: its text size is set here.
            .text_size(crate::theme::rem(13.))
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_strong())
            // A menu-like list: GitHub's overlay, its text bright.
            .bg(theme::overlay())
            .text_color(theme::fg())
            .shadow_lg()
            .flex()
            .flex_col()
            .children(
                self.0
                    .iter()
                    .map(|pr| crate::gh_rows::pr_row(pr, crate::gh_rows::Fold::Always, None)),
            )
    }
}

#[cfg(test)]
mod tests {
    use leon_domain::{PrState, PullRequest};

    fn pr(state: PrState) -> PullRequest {
        PullRequest {
            repo: "acme/api".into(),
            number: 1,
            title: String::new(),
            url: String::new(),
            state,
            branch: String::new(),
            base: String::new(),
            labels: vec![],
            checks: vec![],
            rollup: None,
            reviewers: vec![],
            comments: vec![],
            body: String::new(),
            threads: vec![],
            author: String::new(),
            closes: vec![],
            lines: None,
            conflicts: false,
        }
    }

    #[test]
    fn only_a_blocked_pull_request_is_marked() {
        use leon_domain::{Check, CheckState};
        let checked = |state, check| PullRequest {
            checks: vec![Check {
                name: "test".into(),
                state: check,
                url: None,
            }],
            ..pr(state)
        };
        let conflicting = PullRequest {
            conflicts: true,
            ..pr(PrState::Open)
        };
        let cases = [
            (checked(PrState::Open, CheckState::Pass), false),
            (checked(PrState::Open, CheckState::Pending), false),
            (checked(PrState::Open, CheckState::Fail), true),
            (checked(PrState::Draft, CheckState::Fail), true),
            (checked(PrState::Merged, CheckState::Fail), false),
            (conflicting, true),
        ];
        for (pr, marked) in cases {
            assert_eq!(crate::pr_view::blocked(&pr), marked, "{:?}", pr.state);
        }
    }
}
