//! The doctor view: what works, what is missing, and what to do, like
//! `flutter doctor`. Opened from the bottom of the left pane.

use gpui::{AnyElement, Context, FontWeight, IntoElement, Rgba, div, prelude::*};
use leon_core::Intent;
use leon_domain::{DoctorSection, Health};

use crate::icons::{Icon, icon};
use crate::theme;
use crate::widgets::{button, empty, pane_header};
use crate::workspace::Workspace;

fn mark(health: Health) -> (Icon, Rgba) {
    match health {
        Health::Ok => (Icon::Passed, theme::ok()),
        Health::Warning => (Icon::Info, theme::warn()),
        Health::Problem => (Icon::Failed, theme::err()),
        Health::Info => (Icon::Info, theme::fg_faint()),
    }
}

/// Problems and warnings in a report.
fn counts(sections: &[DoctorSection]) -> (usize, usize) {
    let count = |h| {
        sections
            .iter()
            .flat_map(|s| &s.checks)
            .filter(|c| c.health == h)
            .count()
    };
    (count(Health::Problem), count(Health::Warning))
}

/// The colour of a report's worst finding.
pub(crate) fn worst(sections: &[DoctorSection]) -> Rgba {
    match counts(sections) {
        (0, 0) => theme::ok(),
        (0, _) => theme::warn(),
        _ => theme::err(),
    }
}

fn summary(problems: usize, warnings: usize) -> String {
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    match (problems, warnings) {
        (0, 0) => "Everything works".to_owned(),
        (0, w) => plural(w, "warning"),
        (p, 0) => plural(p, "problem"),
        (p, w) => format!("{}, {}", plural(p, "problem"), plural(w, "warning")),
    }
}

impl Workspace {
    /// The report in the main pane.
    pub(crate) fn render_doctor(&self, cx: &mut Context<Self>) -> AnyElement {
        let running = self.snapshot.doctor_running;
        let subtitle = self.snapshot.doctor.as_ref().map(|(_, sections)| {
            let (p, w) = counts(sections);
            summary(p, w)
        });
        let header = pane_header("Doctor", subtitle).child(
            button(
                "doctor-run",
                if running { "Checking…" } else { "Run again" },
                cx.listener(|this, _, _, _| this.core.send(Intent::RunDoctor)),
            )
            .when(running, |b| b.opacity(0.6)),
        );
        let Some((_, sections)) = &self.snapshot.doctor else {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .child(header)
                .child(empty("Checking what Leon needs…"))
                .into_any_element();
        };
        let body = report(sections);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("doctor-body")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_6()
                    .py_4()
                    .text_size(crate::theme::rem(13.))
                    .child(crate::widgets::centered(
                        820.,
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(self.limits_card())
                            .children(body),
                    )),
            )
            .into_any_element()
    }
}

/// A report's sections, each a card: its title with its worst mark, then
/// one row per check — mark, name, detail — and under a problem, what to do
/// in a tinted box.
pub(crate) fn report(sections: &[DoctorSection]) -> Vec<AnyElement> {
    sections
        .iter()
        .map(|section| {
            let (glyph, color) = mark(section.health());
            let rows = section.checks.iter().enumerate().map(|(ix, check)| {
                let (glyph, color) = mark(check.health);
                div()
                    .px_4()
                    .py(crate::theme::rem(8.))
                    .flex()
                    .flex_col()
                    .gap(crate::theme::rem(6.))
                    .when(ix > 0, |d| d.border_t_1().border_color(theme::border()))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap_3()
                            .child(
                                div()
                                    .pt(crate::theme::rem(1.))
                                    .child(icon(glyph, color, 15.)),
                            )
                            .child(
                                div()
                                    .w(crate::theme::rem(170.))
                                    .flex_none()
                                    .text_color(theme::fg())
                                    .child(check.name.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_color(theme::fg_muted())
                                    .child(check.detail.clone()),
                            ),
                    )
                    .children(check.fix.as_ref().map(|fix| {
                        div()
                            .ml(crate::theme::rem(27.))
                            .px_3()
                            .py(crate::theme::rem(6.))
                            .rounded_md()
                            .bg(Rgba { a: 0.12, ..color })
                            .border_1()
                            .border_color(Rgba { a: 0.35, ..color })
                            .text_size(crate::theme::rem(12.5))
                            .child(fix.clone())
                    }))
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
                        .child(icon(glyph, color, 16.))
                        .child(
                            div()
                                .text_size(crate::theme::rem(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(section.title.clone()),
                        ),
                )
                .children(rows)
                .into_any_element()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use leon_domain::{DoctorCheck, DoctorSection, Health};

    use super::{counts, summary};

    #[test]
    fn the_summary_counts_problems_and_warnings() {
        let sections = [DoctorSection {
            title: "Codex".into(),
            checks: vec![
                DoctorCheck::new(Health::Problem, "Logged in", "no"),
                DoctorCheck::new(Health::Warning, "Version", "old"),
                DoctorCheck::new(Health::Info, "Path", "/x"),
            ],
        }];
        assert_eq!(counts(&sections), (1, 1));
        assert_eq!(summary(1, 1), "1 problem, 1 warning");
        assert_eq!(summary(0, 2), "2 warnings");
        assert_eq!(summary(0, 0), "Everything works");
    }
}
