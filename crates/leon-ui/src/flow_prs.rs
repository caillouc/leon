//! A flow's pull requests in its left-pane row (NAV-6): their icon and how
//! many, and on hover the list of them — title, state, repository and
//! number, lines added and removed — each opening Leon's view of it.

use gpui::{
    AnyElement, Context, Div, IntoElement, Render, SharedString, Stateful, Window, div, prelude::*,
};
use leon_domain::{CheckState, FlowId, GhRef, PrState, PullRequest};

use crate::diff_rows::counts;
use crate::github::pr_icon;
use crate::markdown::Core;
use crate::pr_view::{check_color, pr_state};
use crate::theme;

/// The state shown for several: the most active (open, then draft, merged,
/// closed).
fn lead(prs: &[PullRequest]) -> PrState {
    prs.iter().map(|p| p.state).min().unwrap_or(PrState::Closed)
}

/// The icon in the leading state's colour, how many, and the checks of the
/// open ones as a dot; `None` without pull requests.
pub(crate) fn flow_prs(id: FlowId, prs: Vec<PullRequest>) -> Option<AnyElement> {
    if prs.is_empty() {
        return None;
    }
    let state = lead(&prs);
    let checks = CheckState::worst(
        prs.iter()
            .filter(|p| matches!(p.state, PrState::Open | PrState::Draft))
            .filter_map(PullRequest::checks_state),
    );
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
            .children(checks.map(|s| {
                div()
                    .size(crate::theme::rem(6.))
                    .rounded_full()
                    .bg(check_color(s))
            }))
            .hoverable_tooltip(move |_, cx| cx.new(|_| PrList(prs.clone())).into())
            .into_any_element(),
    )
}

/// The list shown on hover.
struct PrList(Vec<PullRequest>);

impl Render for PrList {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(crate::theme::rem(380.))
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_strong())
            // Lighter than the panes it opens over, its text bright.
            .bg(theme::surface_2())
            .text_color(theme::fg())
            .shadow_lg()
            .flex()
            .flex_col()
            .children(self.0.iter().enumerate().map(|(ix, pr)| row(ix, pr)))
    }
}

/// One pull request: its title, then its state, where it is and its lines.
fn row(ix: usize, pr: &PullRequest) -> Stateful<Div> {
    let item = GhRef::pull_request(&pr.repo, pr.number);
    let (color, state) = pr_state(pr.state);
    let ellipsis = |d: Div| {
        d.min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
    };
    div()
        .id(ix)
        .px_3()
        .py(crate::theme::rem(6.))
        .rounded_md()
        .cursor_pointer()
        .hover(|s| s.bg(theme::accent_soft()))
        .flex()
        .flex_col()
        .gap(crate::theme::rem(2.))
        .child(
            ellipsis(div().text_size(crate::theme::rem(13.)))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(pr.title.clone()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(crate::theme::rem(5.))
                .text_size(crate::theme::rem(12.))
                .child(pr_icon(pr.state, 12.))
                .child(div().flex_none().text_color(color).child(state))
                .child(div().text_color(theme::fg_faint()).child("·"))
                .child(
                    ellipsis(div().flex_1())
                        .text_color(theme::fg_muted())
                        .child(format!(
                            "{} #{}",
                            leon_domain::repo_name(&pr.repo),
                            pr.number
                        )),
                )
                .children(pr.lines.map(|(added, removed)| counts(added, removed))),
        )
        .on_click(move |_, _, cx| {
            if let Some(core) = cx.try_global::<Core>() {
                core.0
                    .send(leon_core::Intent::ShowDetail(Some(item.clone())));
            }
        })
}
