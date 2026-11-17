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
    pub labels: Vec<String>,
    /// CI checks.
    pub checks: Vec<Check>,
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
            return None;
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
    pub labels: Vec<String>,
    /// Last update, Unix milliseconds.
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::{Check, CheckState, PrState, PullRequest};

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
    }
}
