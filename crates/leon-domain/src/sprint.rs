//! Sprints: the iterations of a GitHub Project board (GH-7).

use crate::{Day, Issue};

/// An iteration (a sprint) an issue is planned in, from an iteration field
/// of its GitHub Project board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iteration {
    /// The field, e.g. "Sprint".
    pub field: String,
    /// The iteration, e.g. "Sprint 12".
    pub title: String,
    /// Its first day.
    pub start: Day,
    /// How many days it lasts.
    pub days: u32,
}

impl Iteration {
    /// Its last day.
    pub fn last_day(&self) -> Day {
        self.start.plus(i64::from(self.days.max(1)) - 1)
    }

    /// Whether `today` falls in it.
    pub fn is_current(&self, today: Day) -> bool {
        self.start <= today && today <= self.last_day()
    }
}

/// The sprint `today` falls in, among the issues' iterations. With several
/// iteration fields (a sprint and a quarter), the shortest wins.
pub fn current_sprint(issues: &[Issue], today: Day) -> Option<&Iteration> {
    issues
        .iter()
        .flat_map(|i| &i.iterations)
        .filter(|i| i.is_current(today))
        .min_by(|a, b| a.days.cmp(&b.days).then_with(|| a.title.cmp(&b.title)))
}

#[cfg(test)]
mod tests {
    use super::{Iteration, current_sprint};
    use crate::{Day, Issue};

    fn sprint(field: &str, title: &str, start: &str, days: u32) -> Iteration {
        Iteration {
            field: field.into(),
            title: title.into(),
            start: Day::parse(start).unwrap(),
            days,
        }
    }

    fn issue(iterations: Vec<Iteration>) -> Issue {
        Issue {
            repo: "o/r".into(),
            number: 1,
            title: String::new(),
            url: String::new(),
            labels: vec![],
            updated_at: 0,
            status: None,
            priority: None,
            iterations,
            assignees: vec![],
            body: String::new(),
            comments: vec![],
        }
    }

    #[test]
    fn the_current_sprint_is_the_one_today_falls_in() {
        let s23 = sprint("Sprint", "Sprint 23", "2026-11-09", 12);
        let s24 = sprint("Sprint", "Sprint 24", "2026-11-23", 14);
        let quarter = sprint("Quarter", "Q4", "2026-10-01", 92);
        let issues = [
            issue(vec![s23.clone(), quarter.clone()]),
            issue(vec![s24.clone()]),
        ];
        let current =
            |day: &str| current_sprint(&issues, Day::parse(day).unwrap()).map(|i| i.title.as_str());
        let cases = [
            ("2026-11-09", Some("Sprint 23")),
            ("2026-11-18", Some("Sprint 23")),
            // Its 12th and last day.
            ("2026-11-20", Some("Sprint 23")),
            // Between two sprints, only the quarter.
            ("2026-11-21", Some("Q4")),
            ("2026-12-06", Some("Sprint 24")),
            ("2026-12-07", Some("Q4")),
            ("2027-01-01", None),
        ];
        for (day, expected) in cases {
            assert_eq!(current(day), expected, "{day}");
        }
        assert_eq!(s23.last_day(), Day::parse("2026-11-20").unwrap());
        assert_eq!(current_sprint(&[], Day(0)), None);
    }
}
