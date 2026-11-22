//! How a pull request shows wherever it is named (GH-1, GH-4): its state's
//! colour and word, its checks, its labels.

use gpui::{Div, Rgba, div, prelude::*};
use leon_domain::{CheckState, Label, PrState, PullRequest};

use crate::sprints::tinted_chip;
use crate::theme;

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
        CheckState::Skipped => theme::fg_muted(),
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

/// Something stops an open pull request: a check failed, or it conflicts
/// with its base. Only then a small PR indicator carries a mark: passing
/// or running checks show nothing (NAV-6).
pub(crate) fn blocked(pr: &PullRequest) -> bool {
    matches!(pr.state, PrState::Open | PrState::Draft)
        && (pr.conflicts || pr.checks_state() == Some(CheckState::Fail))
}
