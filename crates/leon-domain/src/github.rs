//! GitHub objects Leon shows (NAV-6, GH-*), independent of the API.

/// State of a pull request (NAV-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    /// Open, ready for review.
    Open,
    /// Open as a draft.
    Draft,
    /// Merged.
    Merged,
    /// Closed without merging.
    Closed,
}

/// Outcome of a check run (GH-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    /// Passed (or skipped / neutral).
    Pass,
    /// Failed, cancelled or timed out.
    Fail,
    /// Queued or running.
    Pending,
}

/// One CI check of a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Check name.
    pub name: String,
    /// Outcome.
    pub state: CheckState,
}

/// A label (GH-4), with GitHub's own color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// Name.
    pub name: String,
    /// Six hex digits, e.g. `d73a4a`.
    pub color: String,
}

/// Where an issue stands on its GitHub Project board: the board's "Status".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueStatus {
    /// The column, e.g. "In progress".
    pub name: String,
    /// Its place among the board's columns, for grouping in the board's order.
    pub order: usize,
    /// GitHub's color name for it (`GRAY`, `BLUE`, `GREEN`…), if any.
    pub color: Option<String>,
}

/// A comment on a pull request or an issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// Author login.
    pub author: String,
    /// Markdown body.
    pub body: String,
    /// When, Unix milliseconds.
    pub at: i64,
}

/// A pull request with what the right pane shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    /// `owner/name`.
    pub repo: String,
    /// Number.
    pub number: u64,
    /// Title.
    pub title: String,
    /// Web URL.
    pub url: String,
    /// State.
    pub state: PrState,
    /// Head branch.
    pub branch: String,
    /// Base branch.
    pub base: String,
    /// Labels (GH-4).
    pub labels: Vec<Label>,
    /// CI checks.
    pub checks: Vec<Check>,
    /// The checks summed up by GitHub, when the checks themselves were not
    /// read (lists).
    pub rollup: Option<CheckState>,
    /// Requested or past reviewers, by login.
    pub reviewers: Vec<String>,
    /// Comments, oldest first.
    pub comments: Vec<Comment>,
}

impl PullRequest {
    /// The checks summed up: failing if any fails, pending if any runs,
    /// passing otherwise; `None` without checks.
    pub fn checks_state(&self) -> Option<CheckState> {
        if self.checks.is_empty() {
            return self.rollup;
        }
        let any = |s| self.checks.iter().any(|c| c.state == s);
        Some(if any(CheckState::Fail) {
            CheckState::Fail
        } else if any(CheckState::Pending) {
            CheckState::Pending
        } else {
            CheckState::Pass
        })
    }
}

/// An issue (GH-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// `owner/name`.
    pub repo: String,
    /// Number.
    pub number: u64,
    /// Title.
    pub title: String,
    /// Web URL.
    pub url: String,
    /// Labels (GH-4).
    pub labels: Vec<Label>,
    /// Last update, Unix milliseconds.
    pub updated_at: i64,
    /// Its column on a GitHub Project board, if it is on one.
    pub status: Option<IssueStatus>,
}

/// Issues grouped by their board column, in the board's order; issues on
/// no board come last, under `None`. Order inside a group is kept.
pub fn issues_by_status(issues: &[Issue]) -> Vec<(Option<&IssueStatus>, Vec<&Issue>)> {
    let mut groups: Vec<(Option<&IssueStatus>, Vec<&Issue>)> = Vec::new();
    for issue in issues {
        let key = issue.status.as_ref();
        let same = |s: Option<&IssueStatus>| s.map(|s| &s.name) == key.map(|k| &k.name);
        match groups.iter_mut().find(|(s, _)| same(*s)) {
            Some((_, members)) => members.push(issue),
            None => groups.push((key, vec![issue])),
        }
    }
    groups.sort_by_key(|(s, _)| s.map_or(usize::MAX, |s| s.order));
    groups
}

#[cfg(test)]
mod tests {
    use super::{Check, CheckState, Issue, IssueStatus, PrState, PullRequest, issues_by_status};

    fn pr(states: &[CheckState]) -> PullRequest {
        PullRequest {
            repo: "o/r".into(),
            number: 1,
            title: "t".into(),
            url: String::new(),
            state: PrState::Open,
            branch: "b".into(),
            base: "main".into(),
            labels: vec![],
            rollup: None,
            checks: states
                .iter()
                .map(|s| Check {
                    name: "c".into(),
                    state: *s,
                })
                .collect(),
            reviewers: vec![],
            comments: vec![],
        }
    }

    #[test]
    fn checks_sum_up() {
        assert_eq!(pr(&[]).checks_state(), None);
        assert_eq!(
            pr(&[CheckState::Pass, CheckState::Pass]).checks_state(),
            Some(CheckState::Pass)
        );
        assert_eq!(
            pr(&[CheckState::Pass, CheckState::Pending]).checks_state(),
            Some(CheckState::Pending)
        );
        assert_eq!(
            pr(&[CheckState::Pending, CheckState::Fail]).checks_state(),
            Some(CheckState::Fail)
        );
        let listed = PullRequest {
            rollup: Some(CheckState::Pending),
            ..pr(&[])
        };
        assert_eq!(listed.checks_state(), Some(CheckState::Pending));
    }

    #[test]
    fn issues_group_in_board_order_with_no_status_last() {
        type Groups<'a> = Vec<(Option<&'a str>, Vec<u64>)>;
        let issue = |number, status: Option<(&str, usize)>| Issue {
            repo: "o/r".into(),
            number,
            title: String::new(),
            url: String::new(),
            labels: vec![],
            updated_at: 0,
            status: status.map(|(name, order)| IssueStatus {
                name: name.into(),
                order,
                color: None,
            }),
        };
        let cases: [(Vec<Issue>, Groups); 3] = [
            (vec![], vec![]),
            (
                vec![issue(1, None), issue(2, None)],
                vec![(None, vec![1, 2])],
            ),
            (
                vec![
                    issue(1, None),
                    issue(2, Some(("Done", 3))),
                    issue(3, Some(("Todo", 0))),
                    issue(4, Some(("In progress", 1))),
                    issue(5, Some(("Todo", 0))),
                ],
                vec![
                    (Some("Todo"), vec![3, 5]),
                    (Some("In progress"), vec![4]),
                    (Some("Done"), vec![2]),
                    (None, vec![1]),
                ],
            ),
        ];
        for (issues, expected) in cases {
            let got: Vec<_> = issues_by_status(&issues)
                .into_iter()
                .map(|(s, members)| {
                    (
                        s.map(|s| s.name.as_str()),
                        members.iter().map(|i| i.number).collect::<Vec<_>>(),
                    )
                })
                .collect();
            assert_eq!(got, expected);
        }
    }
}
