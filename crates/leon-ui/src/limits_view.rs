//! The CLIs' plan limits, first in Doctor (DOCTOR-2): for each CLI that
//! said how much of its plan is used, one bar per window (5 hours, a
//! week…), its share, and when it starts again; then the credits its
//! account has left, when it says (USE-1). A card like Doctor's others.

use gpui::{AnyElement, Div, FontWeight, IntoElement, Rgba, div, prelude::*, relative};
use leon_domain::{LimitWindow, window_name};

use crate::icons::{Icon, icon};
use crate::theme;
use crate::workspace::Workspace;

impl Workspace {
    /// The "Usage limits" card, always first in Doctor: each CLI's bars once
    /// it has said its limits, else when they will show.
    pub(crate) fn limits_card(&self) -> AnyElement {
        let now = crate::left::now_ms();
        let usage = &self.snapshot.usage;
        let rows = usage.iter().enumerate().map(|(ix, u)| {
            div()
                .px_4()
                .py(theme::rem(8.))
                .flex()
                .items_start()
                .gap_3()
                .when(ix > 0, |d| d.border_t_1().border_color(theme::border()))
                .child(
                    div()
                        .w(theme::rem(197.))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_color(theme::fg())
                                .child(u.backend.display_name()),
                        )
                        // When it said it: kept across restarts.
                        .child(
                            div()
                                .text_size(theme::rem(11.5))
                                .text_color(theme::fg_faint())
                                .child(format!("said {}", crate::left::since(u.at))),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(theme::rem(6.))
                        .children(u.windows.iter().map(|w| window_row(w, now)))
                        // Credits its account has left, when it says (USE-1).
                        .children(u.credits.as_ref().map(|c| {
                            div()
                                .text_color(theme::fg_muted())
                                .child(format!("Credits: {c} left"))
                        })),
                )
        });
        let waiting = usage.is_empty().then(|| {
            div()
                .px_4()
                .py(theme::rem(8.))
                .text_size(theme::rem(12.5))
                .text_color(theme::fg_faint())
                .child(
                    "A CLI says how much of your plan is used: Codex when Doctor runs, \
                     Claude Code during a turn. Copilot CLI does not say it.",
                )
        });
        div()
            .rounded_xl()
            .border_1()
            .border_color(theme::border())
            .bg(theme::panel())
            .overflow_hidden()
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(icon(Icon::Info, theme::fg_muted(), 16.))
                    .child(
                        div()
                            .text_size(theme::rem(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Usage limits"),
                    )
                    .child(
                        div()
                            .text_size(theme::rem(12.))
                            .text_color(theme::fg_faint())
                            .child("as each CLI last said"),
                    ),
            )
            .children(rows)
            .children(waiting)
            .into_any_element()
    }
}

/// One window: its name, a bar, its share and the time until it resets.
fn window_row(window: &LimitWindow, now: i64) -> Div {
    let used = used_now(window, now);
    let color = level(used);
    div()
        .flex()
        .items_center()
        .gap_3()
        .text_color(theme::fg_muted())
        .child(
            div()
                .w(theme::rem(56.))
                .flex_none()
                .child(window_name(window.minutes)),
        )
        .child(
            div()
                .flex_1()
                .h(theme::rem(5.))
                .rounded_full()
                .bg(theme::surface_2())
                .child(div().h_full().rounded_full().bg(color).w(relative(
                    f32::from(u16::try_from(used.min(100)).unwrap_or(100)) / 100.,
                ))),
        )
        .child(
            div()
                .flex_none()
                .w(theme::rem(40.))
                .flex()
                .justify_end()
                .text_color(if used >= 75 { color } else { theme::fg_muted() })
                .child(format!("{used} %")),
        )
        .children(window.resets_at.filter(|at| *at > now).map(|at| {
            div()
                .flex_none()
                .text_color(theme::fg_faint())
                .child(until(at, now))
        }))
}

/// How much of the window is used now: nothing once it has started again.
fn used_now(window: &LimitWindow, now: i64) -> u32 {
    if window.resets_at.is_some_and(|at| at <= now) {
        0
    } else {
        window.used
    }
}

/// The bar's colour: amber from 75 %, red from 90 %.
fn level(used: u32) -> Rgba {
    match used {
        90.. => theme::err(),
        75..=89 => theme::warn(),
        _ => theme::accent(),
    }
}

/// Time until `at`, in the app's short units: "in 35m", "in 4h", "in 2d".
fn until(at: i64, now: i64) -> String {
    let minutes = (at - now).max(0) / 60_000;
    match minutes {
        m if m < 60 => format!("in {m}m"),
        m if m < 48 * 60 => format!("in {}h", m / 60),
        m => format!("in {}d", m / (24 * 60)),
    }
}

#[cfg(test)]
mod tests {
    use leon_domain::LimitWindow;

    use super::{level, until, used_now};
    use crate::theme;

    #[test]
    fn a_window_says_when_it_starts_again() {
        let hour = 3_600_000;
        let cases = [
            (35 * 60_000, "in 35m"),
            (4 * hour, "in 4h"),
            (47 * hour, "in 47h"),
            (72 * hour, "in 3d"),
        ];
        for (left, text) in cases {
            assert_eq!(until(1_000 + left, 1_000), text, "{left}");
        }
    }

    #[test]
    fn a_window_past_its_reset_is_empty_again() {
        let window = LimitWindow {
            minutes: 300,
            used: 83,
            resets_at: Some(5_000),
        };
        assert_eq!(used_now(&window, 4_000), 83);
        assert_eq!(used_now(&window, 5_000), 0);
    }

    #[test]
    fn the_bar_warns_near_the_limit() {
        let cases = [
            (0, theme::accent()),
            (74, theme::accent()),
            (75, theme::warn()),
            (90, theme::err()),
            (130, theme::err()),
        ];
        for (used, color) in cases {
            assert_eq!(level(used), color, "{used}");
        }
    }
}
