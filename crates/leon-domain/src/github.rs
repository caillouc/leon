//! GitHub objects Leon shows (NAV-6, GH-*), independent of the API.

use crate::Iteration;

/// State of a pull request (NAV-6), ordered from the most active: open,
/// draft, merged, closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

impl CheckState {
    /// Several states summed up: failing if any fails, pending if any
    /// runs, passing otherwise; `None` without any.
    pub fn worst(states: impl IntoIterator<Item = Self>) -> Option<Self> {
        states.into_iter().max_by_key(|s| match s {
            Self::Pass => 0,
            Self::Pending => 1,
            Self::Fail => 2,
        })
    }
}

/// One CI check of a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// Check name.
    pub name: String,
    /// Outcome.
    pub state: CheckState,
    /// Its run on GitHub (an Actions job's log), when GitHub gives one.
    pub url: Option<String>,
}

/// Which kind of GitHub item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GhKind {
    /// An issue.
    Issue,
    /// A pull request.
    PullRequest,
}

/// An issue or a pull request, by repository and number.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GhRef {
    /// Issue or pull request.
    pub kind: GhKind,
    /// `owner/name`.
    pub repo: String,
    /// Number.
    pub number: u64,
}

impl GhRef {
    /// An issue.
    pub fn issue(repo: &str, number: u64) -> Self {
        Self {
            kind: GhKind::Issue,
            repo: repo.to_owned(),
            number,
        }
    }

    /// A pull request.
    pub fn pull_request(repo: &str, number: u64) -> Self {
        Self {
            kind: GhKind::PullRequest,
            repo: repo.to_owned(),
            number,
        }
    }

    /// `owner/name#12`.
    pub fn text(&self) -> String {
        format!("{}#{}", self.repo, self.number)
    }

    /// `name#12`, as the UI shows it: the repository without its owner.
    pub fn short(&self) -> String {
        format!("{}#{}", repo_name(&self.repo), self.number)
    }

    /// The issue `owner/name#12` names.
    pub fn issue_from_text(text: &str) -> Option<Self> {
        let (repo, number) = text.rsplit_once('#')?;
        (repo.contains('/')).then_some(())?;
        Some(Self::issue(repo, number.parse().ok()?))
    }

    /// The issue or pull request a GitHub page's address names:
    /// `https://github.com/owner/name/issues/12` or `…/pull/12`, with or
    /// without a tab, a query or an anchor after it.
    pub fn from_url(url: &str) -> Option<Self> {
        let rest = url
            .strip_prefix("https://github.com/")
            .or_else(|| url.strip_prefix("http://github.com/"))?;
        let rest = rest.split(['?', '#']).next()?;
        let mut parts = rest.split('/');
        let (owner, name, kind, number) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        if owner.is_empty() || name.is_empty() {
            return None;
        }
        let repo = format!("{owner}/{name}");
        let number = number.parse().ok()?;
        match kind {
            "issues" => Some(Self::issue(&repo, number)),
            "pull" => Some(Self::pull_request(&repo, number)),
            _ => None,
        }
    }
}

/// A repository's name without its owner: `api` for `acme/api`, as the UI
/// shows it.
pub fn repo_name(repo: &str) -> &str {
    repo.rsplit_once('/').map_or(repo, |(_, name)| name)
}

/// A label (GH-4), with GitHub's own color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// Name.
    pub name: String,
    /// Six hex digits, e.g. `d73a4a`.
    pub color: String,
}

/// An issue's value in a single-select field of its GitHub Project board,
/// such as "Status" (the board's column) or "Priority".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardOption {
    /// The option, e.g. "In progress" or "P1".
    pub name: String,
    /// Its place among the field's options, for the board's order.
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
    /// Markdown description. Read with the pull request alone (details).
    pub body: String,
    /// Review threads not resolved yet, oldest first. Read with the pull
    /// request alone (details).
    pub threads: Vec<ReviewThread>,
    /// Who opened it, by login; empty when GitHub did not say.
    pub author: String,
    /// The issues it closes when merged, as GitHub links them (CTX-2).
    pub closes: Vec<GhRef>,
    /// Lines added and removed, when GitHub gave them.
    pub lines: Option<(u32, u32)>,
    /// GitHub says its branch conflicts with its base: it cannot be merged
    /// until they are resolved (GH-3).
    pub conflicts: bool,
}

/// A review thread on a pull request's code, not resolved yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewThread {
    /// GitHub's node id, to answer or resolve it (GH-8).
    pub id: String,
    /// The user may resolve it.
    pub can_resolve: bool,
    /// File it is on.
    pub path: String,
    /// Line it is on, when GitHub still knows it.
    pub line: Option<u32>,
    /// Its comments, oldest first.
    pub comments: Vec<Comment>,
    /// The last lines of the code it is on, the commented line last, each
    /// with its diff sign (`+`, `-` or a space); empty when unknown.
    pub code: Vec<String>,
}

impl PullRequest {
    /// The checks summed up: failing if any fails, pending if any runs,
    /// passing otherwise; `None` without checks.
    pub fn checks_state(&self) -> Option<CheckState> {
        if self.checks.is_empty() {
            return self.rollup;
        }
        CheckState::worst(self.checks.iter().map(|c| c.state))
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
    /// Its column on a GitHub Project board (the "Status" field), if any.
    pub status: Option<BoardOption>,
    /// Its "Priority" on the board, if any.
    pub priority: Option<BoardOption>,
    /// The iterations it is planned in, one per iteration field.
    pub iterations: Vec<Iteration>,
    /// Assignees, by login. Read with the issue alone (details).
    pub assignees: Vec<String>,
    /// Markdown body. Read with the issue alone (details).
    pub body: String,
    /// Comments, oldest first. Read with the issue alone (details).
    pub comments: Vec<Comment>,
}

/// Issues grouped by their board column, in the board's order; issues on
/// no board come last, under `None`. Inside a group, by priority (in the
/// board's order, none last), else in the order given.
pub fn issues_by_status(issues: &[Issue]) -> Vec<(Option<&BoardOption>, Vec<&Issue>)> {
    let mut groups: Vec<(Option<&BoardOption>, Vec<&Issue>)> = Vec::new();
    for issue in issues {
        let key = issue.status.as_ref();
        let same = |s: Option<&BoardOption>| s.map(|s| &s.name) == key.map(|k| &k.name);
        match groups.iter_mut().find(|(s, _)| same(*s)) {
            Some((_, members)) => members.push(issue),
            None => groups.push((key, vec![issue])),
        }
    }
    groups.sort_by_key(|(s, _)| s.map_or(usize::MAX, |s| s.order));
    for (_, members) in &mut groups {
        members.sort_by_key(|i| i.priority.as_ref().map_or(usize::MAX, |p| p.order));
    }
    groups
}

/// Pull requests grouped by repository (GH-3), repositories by name, each
/// keeping the order given.
pub fn prs_by_repo(prs: &[PullRequest]) -> Vec<(&str, Vec<&PullRequest>)> {
    let mut groups: std::collections::BTreeMap<&str, Vec<&PullRequest>> =
        std::collections::BTreeMap::new();
    for pr in prs {
        groups.entry(pr.repo.as_str()).or_default().push(pr);
    }
    groups.into_iter().collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_ui_shows_a_repository_without_its_owner() {
        assert_eq!(super::repo_name("acme/payments-api"), "payments-api");
        assert_eq!(super::repo_name("payments-api"), "payments-api");
        assert_eq!(
            super::GhRef::pull_request("acme/payments-api", 412).short(),
            "payments-api#412"
        );
    }

    #[test]
    fn github_addresses_name_issues_and_pull_requests() {
        use super::GhRef;
        let cases = [
            (
                "https://github.com/acme/api/issues/12",
                Some(GhRef::issue("acme/api", 12)),
            ),
            (
                "https://github.com/acme/api/pull/7/files",
                Some(GhRef::pull_request("acme/api", 7)),
            ),
            (
                "https://github.com/acme/api/pull/7#discussion_r1",
                Some(GhRef::pull_request("acme/api", 7)),
            ),
            (
                "https://github.com/acme/api/issues/12?q=1",
                Some(GhRef::issue("acme/api", 12)),
            ),
            ("https://github.com/acme/api/blob/main/a.rs", None),
            ("https://github.com/acme/api/pull/new", None),
            ("https://example.com/acme/api/pull/7", None),
        ];
        for (url, expected) in cases {
            assert_eq!(GhRef::from_url(url), expected, "{url}");
        }
    }

    use super::{
        BoardOption, Check, CheckState, Issue, PrState, PullRequest, issues_by_status, prs_by_repo,
    };

    #[test]
    fn pull_requests_group_by_repository() {
        let pr = |repo: &str, number| PullRequest {
            repo: repo.into(),
            number,
            ..pr(&[])
        };
        let prs = [pr("acme/web", 1), pr("acme/api", 2), pr("acme/web", 3)];
        let got: Vec<(&str, Vec<u64>)> = prs_by_repo(&prs)
            .into_iter()
            .map(|(repo, list)| (repo, list.iter().map(|p| p.number).collect()))
            .collect();
        assert_eq!(got, [("acme/api", vec![2]), ("acme/web", vec![1, 3])]);
        assert_eq!(prs_by_repo(&[]).len(), 0);
    }

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
                    url: None,
                })
                .collect(),
            reviewers: vec![],
            comments: vec![],
            body: String::new(),
            threads: vec![],
            author: String::new(),
            closes: vec![],
            lines: None,
            conflicts: false,
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
            status: status.map(|(name, order)| BoardOption {
                name: name.into(),
                order,
                color: None,
            }),
            priority: None,
            iterations: vec![],
            assignees: vec![],
            body: String::new(),
            comments: vec![],
        };
        let urgent = |number, status| Issue {
            priority: Some(BoardOption {
                name: "P0".into(),
                order: 0,
                color: None,
            }),
            ..issue(number, status)
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
        let prioritised = (
            vec![
                issue(1, Some(("Todo", 0))),
                urgent(2, Some(("Todo", 0))),
                issue(3, Some(("Todo", 0))),
            ],
            vec![(Some("Todo"), vec![2, 1, 3])],
        );
        for (issues, expected) in cases.into_iter().chain([prioritised]) {
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
