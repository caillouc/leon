//! A flow's pull requests in its left-pane row (NAV-6): their icon and how
//! many, and on hover the list of them, one line each (design.md, "PR and
//! issue row", folded), opening Leon's view of it.

use gpui::{AnyElement, Context, IntoElement, Render, Rgba, SharedString, Window, div, prelude::*};
use leon_domain::{CheckState, FlowId, PrState, PullRequest};

use crate::github::pr_glyph;
use crate::icons::icon;
use crate::pr_view::{check_color, pr_state};
use crate::theme;

/// The state shown for several: the most active (open, then draft, merged,
/// closed).
fn lead(prs: &[PullRequest]) -> PrState {
    prs.iter().map(|p| p.state).min().unwrap_or(PrState::Closed)
}

/// The leading state's icon and how many, in its colour, or in red when
/// one of them is blocked (a failed check, a conflict), or in the colour of
/// running checks while an open one's run; `None` without pull requests.
pub(crate) fn flow_prs(id: FlowId, prs: Vec<PullRequest>) -> Option<AnyElement> {
    if prs.is_empty() {
        return None;
    }
    let state = lead(&prs);
    let color = color(&prs, state);
    Some(
        div()
            .id(SharedString::from(format!("flow-prs-{}", id.0)))
            .flex()
            .flex_none()
            .items_center()
            .gap(crate::theme::rem(3.))
            .px(crate::theme::rem(4.))
            .rounded(crate::theme::rem(4.))
            .text_color(color)
            .hover(|s| s.bg(theme::surface_2()))
            .child(icon(pr_glyph(state), color, 13.))
            .child(prs.len().to_string())
            .hoverable_tooltip(move |_, cx| cx.new(|_| PrList(prs.clone())).into())
            .into_any_element(),
    )
}

/// The colour of a flow's pull requests, `state` the leading one's: red
/// when one is blocked, amber while an open one's checks run, else that
/// state's (NAV-6).
fn color(prs: &[PullRequest], state: PrState) -> Rgba {
    let open = |p: &&PullRequest| matches!(p.state, PrState::Open | PrState::Draft);
    if prs.iter().any(crate::pr_view::blocked) {
        theme::err()
    } else if prs
        .iter()
        .filter(open)
        .any(|p| p.checks_state() == Some(CheckState::Pending))
    {
        check_color(CheckState::Pending)
    } else {
        pr_state(state).0
    }
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

    #[test]
    fn the_icon_is_red_when_blocked_amber_while_checks_run() {
        use leon_domain::{Check, CheckState};

        use crate::pr_view::{check_color, pr_state};
        use crate::theme;
        let checked = |state, check| PullRequest {
            checks: vec![Check {
                name: "test".into(),
                state: check,
                url: None,
            }],
            ..pr(state)
        };
        let running = check_color(CheckState::Pending);
        let cases = [
            (
                vec![checked(PrState::Open, CheckState::Pass)],
                pr_state(PrState::Open).0,
            ),
            (vec![checked(PrState::Open, CheckState::Pending)], running),
            (vec![checked(PrState::Draft, CheckState::Pending)], running),
            // A merged one's late checks: its own colour.
            (
                vec![checked(PrState::Merged, CheckState::Pending)],
                pr_state(PrState::Merged).0,
            ),
            // Blocked wins over running.
            (
                vec![
                    checked(PrState::Open, CheckState::Pending),
                    checked(PrState::Open, CheckState::Fail),
                ],
                theme::err(),
            ),
        ];
        for (prs, expected) in cases {
            let state = super::lead(&prs);
            assert_eq!(super::color(&prs, state), expected, "{state:?}");
        }
    }
}
