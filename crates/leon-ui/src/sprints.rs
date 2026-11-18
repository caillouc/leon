//! The sprint filter of my issues, and an issue's board chips: its priority
//! in its board colour, its sprints with the current one highlighted (GH-7).

use std::collections::HashSet;

use gpui::{AnyElement, Context, Div, Rgba, div, prelude::*, px};
use leon_domain::{Day, Issue, Iteration, current_sprint};

use crate::icons::{Icon, icon};
use crate::picker::Choice;
use crate::theme;
use crate::workspace::Workspace;

/// Which sprint the issue list shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SprintFilter {
    /// The sprint today falls in; every issue when there is none.
    Current,
    /// Every issue.
    All,
    /// One sprint: its iteration field and title.
    One(String, String),
}

/// A sprint, as the filter keeps it: (iteration field, title).
pub(crate) type SprintKey = (String, String);

/// The value of the picker's "all" choice.
const ALL: &str = "";

/// Today, in UTC.
pub(crate) fn today() -> Day {
    Day::from_unix_ms(crate::left::now_ms())
}

impl Workspace {
    /// The sprint the list shows; `None` for every issue.
    pub(crate) fn shown_sprint(&self, today: Day) -> Option<SprintKey> {
        match &self.gh.sprint {
            SprintFilter::All => None,
            SprintFilter::One(field, title) => Some((field.clone(), title.clone())),
            SprintFilter::Current => current_sprint(&self.snapshot.my_issues, today)
                .map(|i| (i.field.clone(), i.title.clone())),
        }
    }

    /// The sprint picker: every sprint of my issues with its dates, latest
    /// first; none when no issue is in a sprint.
    pub(crate) fn sprint_filter(
        &self,
        shown: Option<&SprintKey>,
        today: Day,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sprints = sprints(&self.snapshot.my_issues);
        let first = sprints.first()?;
        let fields: HashSet<&str> = sprints.iter().map(|s| s.field.as_str()).collect();
        let with_field = fields.len() > 1;
        let choices: Vec<Choice> = std::iter::once((ALL.to_owned(), "All".to_owned()))
            .chain(sprints.iter().map(|s| {
                (
                    format!("{}\t{}", s.field, s.title),
                    sprint_name(s, today, with_field),
                )
            }))
            .collect();
        let current = shown
            .and_then(|(field, title)| {
                sprints
                    .iter()
                    .find(|s| &s.field == field && &s.title == title)
            })
            .map_or_else(|| "All".to_owned(), |s| sprint_name(s, today, with_field));
        let field = if with_field {
            "Iteration".to_owned()
        } else {
            first.field.clone()
        };
        let picker = self.picker("gh-sprint", &current, choices, cx, |this, value, _, cx| {
            this.gh.sprint = value.split_once('\t').map_or(SprintFilter::All, |(f, t)| {
                SprintFilter::One(f.to_owned(), t.to_owned())
            });
            cx.notify();
        });
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py(px(2.))
                .rounded_md()
                .border_1()
                .border_color(theme::border())
                .text_size(px(12.5))
                .child(icon(Icon::Filter, theme::fg_muted(), 13.))
                .child(div().text_color(theme::fg_muted()).child(field))
                .child(picker)
                .into_any_element(),
        )
    }
}

/// Whether the issue is in the sprint shown; every issue when none is.
pub(crate) fn in_sprint(issue: &Issue, shown: Option<&SprintKey>) -> bool {
    shown.is_none_or(|(field, title)| {
        issue
            .iterations
            .iter()
            .any(|i| &i.field == field && &i.title == title)
    })
}

/// Every sprint of the issues once, latest first.
fn sprints(issues: &[Issue]) -> Vec<&Iteration> {
    let mut sprints: Vec<&Iteration> = issues.iter().flat_map(|i| &i.iterations).collect();
    sprints.sort_by(|a, b| b.start.cmp(&a.start).then(a.title.cmp(&b.title)));
    sprints.dedup_by(|a, b| a.field == b.field && a.title == b.title);
    sprints
}

/// A sprint's first and last days: "Nov 9 – 20", "Nov 23 – Dec 6".
fn dates(sprint: &Iteration) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = |m: i64| {
        usize::try_from(m - 1)
            .ok()
            .and_then(|i| MONTHS.get(i))
            .copied()
            .unwrap_or("?")
    };
    let (_, first_month, first_day) = sprint.start.ymd();
    let (_, last_month, last_day) = sprint.last_day().ymd();
    if sprint.days <= 1 {
        format!("{} {first_day}", month(first_month))
    } else if first_month == last_month {
        format!("{} {first_day} – {last_day}", month(first_month))
    } else {
        format!(
            "{} {first_day} – {} {last_day}",
            month(first_month),
            month(last_month)
        )
    }
}

/// A sprint as the picker shows it: "Sprint 23 · Nov 9 – 20 · current";
/// with several iteration fields, the field first.
fn sprint_name(sprint: &Iteration, today: Day, with_field: bool) -> String {
    let name = if with_field {
        format!("{}: {}", sprint.field, sprint.title)
    } else {
        sprint.title.clone()
    };
    let now = if sprint.is_current(today) {
        " · current"
    } else {
        ""
    };
    format!("{name} · {}{now}", dates(sprint))
}

/// An issue's priority, in its board colour, and its sprints, the current
/// one highlighted.
pub(crate) fn board_chips(issue: &Issue, today: Day) -> Vec<Div> {
    let priority = issue.priority.as_ref().map(|p| {
        let color = p
            .color
            .as_deref()
            .and_then(theme::github_color)
            .unwrap_or_else(theme::fg_muted);
        tinted_chip(&p.name, color)
    });
    let sprints = issue.iterations.iter().map(|i| {
        if i.is_current(today) {
            tinted_chip(&i.title, theme::accent())
        } else {
            chip(&i.title)
        }
    });
    priority.into_iter().chain(sprints).collect()
}

/// A small outlined value: a sprint.
pub(crate) fn chip(text: &str) -> Div {
    div()
        .px(px(7.))
        .rounded_full()
        .border_1()
        .border_color(theme::border_strong())
        .text_size(px(11.))
        .text_color(theme::fg_muted())
        .child(text.to_owned())
}

/// A small value filled with its colour: a label, a priority, the current
/// sprint.
pub(crate) fn tinted_chip(text: &str, color: Rgba) -> Div {
    div()
        .px_2()
        .rounded_full()
        .border_1()
        .border_color(Rgba { a: 0.7, ..color })
        .bg(Rgba { a: 0.18, ..color })
        .text_size(px(11.))
        .text_color(theme::fg())
        .child(text.to_owned())
}

#[cfg(test)]
mod tests {
    use leon_domain::{Day, Iteration};

    use super::{dates, sprint_name};

    fn sprint(start: &str, days: u32) -> Iteration {
        Iteration {
            field: "Sprint".into(),
            title: "Sprint 23".into(),
            start: Day::parse(start).unwrap(),
            days,
        }
    }

    #[test]
    fn sprints_show_their_dates() {
        let cases = [
            (sprint("2026-11-09", 12), "Nov 9 – 20"),
            (sprint("2026-11-23", 14), "Nov 23 – Dec 6"),
            (sprint("2026-12-28", 7), "Dec 28 – Jan 3"),
            (sprint("2026-11-09", 1), "Nov 9"),
        ];
        for (sprint, expected) in cases {
            assert_eq!(dates(&sprint), expected);
        }
        let today = Day::parse("2026-11-18").unwrap();
        let current = sprint("2026-11-09", 12);
        assert_eq!(
            sprint_name(&current, today, false),
            "Sprint 23 · Nov 9 – 20 · current"
        );
        assert_eq!(
            sprint_name(&sprint("2026-11-23", 14), today, true),
            "Sprint: Sprint 23 · Nov 23 – Dec 6"
        );
    }
}
