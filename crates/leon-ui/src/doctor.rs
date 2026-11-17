//! The doctor view: what works, what is missing, and what to do, like
//! `flutter doctor`. Opened from the bottom of the left pane.

use gpui::{AnyElement, Context, FontWeight, IntoElement, Rgba, div, prelude::*, px};
use leon_core::Intent;
use leon_domain::{DoctorSection, Health};

use crate::left::ago;
use crate::theme;
use crate::widgets::{button, empty};
use crate::workspace::{Screen, Workspace};

fn mark(health: Health) -> (&'static str, Rgba) {
    match health {
        Health::Ok => ("✓", theme::ok()),
        Health::Warning => ("!", theme::warn()),
        Health::Problem => ("✕", theme::err()),
        Health::Info => ("·", theme::fg_faint()),
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
    /// The Doctor row at the bottom of the left pane, with a summary.
    pub(crate) fn render_doctor_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.screen == Screen::Doctor;
        let (text, color) = match &self.snapshot.doctor {
            _ if self.snapshot.doctor_running && self.snapshot.doctor.is_none() => {
                ("checking…".to_owned(), theme::fg_faint())
            }
            None => (String::new(), theme::fg_faint()),
            Some((_, sections)) => {
                let (p, w) = counts(sections);
                let color = if p > 0 {
                    theme::err()
                } else if w > 0 {
                    theme::warn()
                } else {
                    theme::ok()
                };
                (summary(p, w), color)
            }
        };
        div()
            .id("doctor-row")
            .flex_none()
            .mx_1()
            .mb_1()
            .px_2()
            .py(px(6.))
            .rounded_md()
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .when(selected, |s| s.bg(theme::accent_soft()))
            .when(!selected, |s| s.hover(|s| s.bg(theme::surface_2())))
            .child(div().flex_1().child("Doctor"))
            .child(div().text_size(px(11.5)).text_color(color).child(text))
            .on_click(cx.listener(|this, _, _, cx| {
                this.screen = Screen::Doctor;
                this.core.send(Intent::View(None));
                this.core.send(Intent::RunDoctor);
                cx.notify();
            }))
            .into_any_element()
    }

    /// The report in the main pane.
    pub(crate) fn render_doctor(&self, cx: &mut Context<Self>) -> AnyElement {
        let running = self.snapshot.doctor_running;
        let header = div()
            .px_6()
            .py_3()
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(theme::border())
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Doctor"),
            )
            .children(self.snapshot.doctor.as_ref().map(|(at, sections)| {
                let (p, w) = counts(sections);
                div()
                    .text_size(px(12.5))
                    .text_color(theme::fg_muted())
                    .child(format!("{} · checked {} ago", summary(p, w), ago(*at)))
            }))
            .child(div().flex_1())
            .child(
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
                    .text_size(px(13.))
                    .child(div().max_w(px(820.)).children(body)),
            )
            .into_any_element()
    }
}

/// A report's sections: a title with its worst mark, then one row per check
/// with its mark, name, detail and fix. Used by the Doctor and the Tools view.
pub(crate) fn report(sections: &[DoctorSection]) -> Vec<AnyElement> {
    sections
        .iter()
        .map(|section| {
            let (glyph, color) = mark(section.health());
            let rows = section.checks.iter().map(|check| {
                let (glyph, color) = mark(check.health);
                div()
                    .flex()
                    .gap_3()
                    .py(px(5.))
                    .child(div().w(px(14.)).flex_none().text_color(color).child(glyph))
                    .child(
                        div()
                            .w(px(180.))
                            .flex_none()
                            .text_color(theme::fg_muted())
                            .child(check.name.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(check.detail.clone())
                            .children(check.fix.as_ref().map(|fix| {
                                div()
                                    .text_size(px(12.))
                                    .text_color(color)
                                    .child(fix.clone())
                            })),
                    )
            });
            div()
                .mb_4()
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .mb_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(div().text_color(color).child(glyph))
                        .child(section.title.clone()),
                )
                .child(div().pl_6().children(rows))
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
