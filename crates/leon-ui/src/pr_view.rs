//! How a pull request shows wherever it is named (GH-1, GH-4): its state's
//! colour and word, its checks, its labels, its badge.

use gpui::{AnyElement, Div, IntoElement, Rgba, div, prelude::*};
use leon_domain::{CheckState, GhRef, Label, PrState, PullRequest};

use crate::sprints::tinted_chip;
use crate::theme;
use crate::workspace::Workspace;

/// Colour and word of a pull request state.
pub(crate) fn pr_state(state: PrState) -> (Rgba, &'static str) {
    match state {
        PrState::Open => (theme::ok(), "Open"),
        PrState::Draft => (theme::fg_muted(), "Draft"),
        PrState::Merged => (theme::merged(), "Merged"),
        PrState::Closed => (theme::err(), "Closed"),
    }
}

/// Colour of a check state.
pub(crate) fn check_color(state: CheckState) -> Rgba {
    match state {
        CheckState::Pass => theme::ok(),
        CheckState::Fail => theme::err(),
        CheckState::Pending => theme::warn(),
    }
}

/// Labels as small pills in GitHub's own colors (GH-4), readable on the
/// dark theme: tinted background, colored border, light text.
pub(crate) fn labels(labels: &[Label]) -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap(crate::theme::rem(5.))
        .children(
            labels
                .iter()
                .map(|l| tinted_chip(&l.name, theme::hex(&l.color).unwrap_or(theme::fg_faint()))),
        )
}

/// A pull request's badge in the left pane (NAV-6): state colour, number,
/// and a dot for its checks while it is open.
/// A pull request's badge: its icon and number in its state's colour, its
/// checks as a dot; a click opens Leon's view of it.
pub(crate) fn pr_badge(pr: &PullRequest, cx: &mut gpui::Context<Workspace>) -> AnyElement {
    let (color, _) = pr_state(pr.state);
    let live = matches!(pr.state, PrState::Open | PrState::Draft);
    let item = GhRef::pull_request(&pr.repo, pr.number);
    div()
        .id(gpui::SharedString::from(format!(
            "pr-badge-{}",
            item.text()
        )))
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_2()))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.core
                .send(leon_core::Intent::ShowDetail(Some(item.clone())));
        }))
        .flex()
        .flex_none()
        .items_center()
        .gap(crate::theme::rem(3.))
        .px(crate::theme::rem(5.))
        .rounded(crate::theme::rem(4.))
        .border_1()
        .border_color(theme::border())
        .font_family(theme::MONO)
        .text_size(crate::theme::rem(10.5))
        .text_color(color)
        // The pull request icon in its state's colour, apart from an issue.
        .child(crate::github::pr_icon(pr.state, 11.))
        .child(format!("#{}", pr.number))
        .children(pr.checks_state().filter(|_| live).map(|s| {
            div()
                .size(crate::theme::rem(6.))
                .rounded_full()
                .bg(check_color(s))
        }))
        .into_any_element()
}
