use leon_domain::{
    BoardOption, Check, CheckState, Comment, Issue, Iteration, Label, PrState, PullRequest,
};
use serde_json::json;

use super::{issue, pull_request, pull_request_by_number, search_issues, search_pull_requests};

fn fixture_pr(json: &str, branch: &str) -> PullRequest {
    pull_request(json, branch).unwrap().unwrap()
}

/// Recorded from gh 2.101.0 on cli/cli (2026-11-17), logins scrubbed.
#[test]
fn recorded_open_pull_request() {
    let branch = "dependabot/npm_and_yarn/evals/zod-4.6.6";
    let pr = fixture_pr(
        include_str!("../../fixtures/gh/2.101.0/pull-request-open.json"),
        branch,
    );
    let check = |name: &str| Check {
        name: name.into(),
        state: CheckState::Pass,
    };
    let expected = PullRequest {
        repo: "cli/cli".into(),
        number: 15086,
        title: "chore(deps): Bump zod from 4.3.6 to 4.6.6 in /evals".into(),
        url: "https://github.com/cli/cli/pull/15086".into(),
        state: PrState::Open,
        branch: branch.into(),
        base: "trunk".into(),
        labels: ["dependencies", "javascript"].map(label).to_vec(),
        checks: vec![
            check("CodeQL-Build (go, manual, ./.github/codeql/codeql-config.yml)"),
            check("label-external / label_issues"),
            check("label-external"),
            check("label-external"),
        ],
        reviewers: vec!["reviewer-a".into(), "copilot-pull-request-reviewer".into()],
        comments: vec![],
        rollup: None,
    };
    assert_eq!(pr, expected);
    assert_eq!(pr.checks_state(), Some(CheckState::Pass));
}

#[test]
fn recorded_merged_pull_request() {
    let pr = fixture_pr(
        include_str!("../../fixtures/gh/2.101.0/pull-request-merged.json"),
        "feature/event-review",
    );
    assert_eq!((pr.number, pr.state), (15083, PrState::Merged));
    assert_eq!(pr.base, "feature/event-check-suite");
    assert_eq!(
        pr.reviewers,
        ["copilot-pull-request-reviewer", "reviewer-b"]
    );
    assert_eq!(
        pr.comments,
        [Comment {
            author: "author-a".into(),
            body: "Leaving the remaining review threads for a follow-up PR.\n".into(),
            at: 1_794_942_161_000,
        }]
    );
    assert_eq!(pr.checks.len(), 2);
}

#[test]
fn recorded_no_pull_request() {
    let json = include_str!("../../fixtures/gh/2.101.0/pull-request-none.json");
    assert_eq!(
        pull_request(json, "leon-fixture-no-such-branch").unwrap(),
        None
    );
}

/// cli/cli's default branch `trunk` matches pull requests from forks.
#[test]
fn default_branch_has_no_pull_request() {
    let json = include_str!("../../fixtures/gh/2.101.0/pull-request-default-branch.json");
    assert_eq!(pull_request(json, "trunk").unwrap(), None);
    assert_eq!(fixture_pr(json, "other").number, 15005);
}

#[test]
fn newest_open_pull_request_first() {
    let node = |number: u64, state: &str, draft: bool| {
        json!({ "number": number, "title": "t", "url": "u", "state": state, "isDraft": draft,
            "headRefName": "b", "baseRefName": "main", "repository": { "nameWithOwner": "o/r" } })
    };
    let answer = |nodes: Vec<serde_json::Value>| {
        json!({ "data": { "repository": { "pullRequests": { "nodes": nodes } } } }).to_string()
    };
    let pick = |nodes| {
        pull_request(&answer(nodes), "b")
            .unwrap()
            .map(|p| (p.number, p.state))
    };
    assert_eq!(
        pick(vec![
            node(3, "CLOSED", false),
            node(2, "OPEN", true),
            node(1, "OPEN", false)
        ]),
        Some((2, PrState::Draft))
    );
    assert_eq!(
        pick(vec![node(3, "MERGED", false), node(2, "CLOSED", false)]),
        Some((3, PrState::Merged))
    );
    assert_eq!(
        pick(vec![
            json!({ "number": 4, "state": "OPEN" }),
            node(1, "OPEN", false)
        ]),
        Some((1, PrState::Open))
    );
    assert_eq!(pick(vec![node(1, "UNKNOWN", false)]), None);
}

#[test]
fn reviewers_and_ghost_comments() {
    let json = json!({ "data": { "repository": { "pullRequests": { "nodes": [{
        "number": 1, "title": "t", "url": "u", "state": "OPEN", "isDraft": false,
        "headRefName": "b", "baseRefName": "main", "repository": { "nameWithOwner": "o/r" },
        "reviewRequests": { "nodes": [
            { "requestedReviewer": null },
            { "requestedReviewer": { "combinedSlug": "org/team" } },
            { "requestedReviewer": { "login": "a" } }
        ] },
        "latestReviews": { "nodes": [{ "author": { "login": "a" } }, { "author": { "login": "b" } }] },
        "comments": { "nodes": [{ "author": null, "body": "hi", "createdAt": "bad" }] }
    }] } } } });
    let pr = fixture_pr(&json.to_string(), "b");
    assert_eq!(pr.reviewers, ["org/team", "a", "b"]);
    assert_eq!(
        pr.comments,
        [Comment {
            author: "ghost".into(),
            body: "hi".into(),
            at: 0
        }]
    );
}

/// A label as recorded before colors were asked for.
fn label(name: &str) -> Label {
    Label {
        name: name.into(),
        color: String::new(),
    }
}

/// The shape of a live `gh api graphql` answer (2026-11-17) with label
/// colors and the check summary; values made up.
#[test]
fn label_colors_and_check_summary() {
    let json = r#"{"data":{"search":{"nodes":[
      {"__typename":"Issue","number":7,"title":"Retry","url":"https://github.com/o/r/issues/7",
       "updatedAt":"2026-11-17T10:00:00Z","repository":{"nameWithOwner":"o/r"},
       "labels":{"nodes":[{"name":"bug","color":"d73a4a"}]}},
      {"__typename":"PullRequest","number":8,"title":"Fix","url":"https://github.com/o/r/pull/8",
       "state":"OPEN","isDraft":false,"headRefName":"fix","baseRefName":"main",
       "repository":{"nameWithOwner":"o/r"},"labels":{"nodes":[]},
       "commits":{"nodes":[{"commit":{"statusCheckRollup":{"state":"FAILURE"}}}]}}
    ]}}}"#;
    let issue = &search_issues(json).unwrap()[0];
    assert_eq!(
        issue.labels,
        [Label {
            name: "bug".into(),
            color: "d73a4a".into()
        }]
    );
    let pr = &search_pull_requests(json).unwrap()[0];
    assert_eq!(pr.checks_state(), Some(CheckState::Fail));
}

fn option(name: &str, order: usize, color: &str) -> BoardOption {
    BoardOption {
        name: name.into(),
        order,
        color: Some(color.into()),
    }
}

/// Recorded from gh 2.101.0 on a private repository (2026-11-18), every
/// name, text and id scrubbed: an issue on three boards, its "Priority" an
/// organization issue field; the first board's values win.
#[test]
fn recorded_issue_on_three_boards() {
    let found = issue(include_str!("../../fixtures/gh/2.101.0/issue.json"))
        .unwrap()
        .unwrap();
    let expected = Issue {
        repo: "acme/api".into(),
        number: 101,
        title: "Retry failed uploads".into(),
        url: "https://github.com/acme/api/issues/101".into(),
        labels: vec![],
        updated_at: 1_794_237_768_000,
        status: Some(option("Backlog", 0, "GREEN")),
        priority: Some(option("P1", 1, "RED")),
        iterations: vec![Iteration {
            field: "Sprint".into(),
            title: "Sprint 40".into(),
            start: "2026-11-23".into(),
        }],
        assignees: vec!["octocat".into()],
        body: "Steps to reproduce:\n\n1. Open the app\n".into(),
        comments: vec![],
    };
    assert_eq!(found, expected);
}

/// Recorded and scrubbed like the one above: an empty body, one comment.
#[test]
fn recorded_issue_with_a_comment() {
    let found = issue(include_str!("../../fixtures/gh/2.101.0/issue-comment.json"))
        .unwrap()
        .unwrap();
    assert_eq!(found.status, Some(option("In review", 3, "PURPLE")));
    assert_eq!(found.body, "");
    assert_eq!(
        found.comments,
        [Comment {
            author: "hubot".into(),
            body: "Reproduced on the latest build.\n".into(),
            at: 1_794_238_669_000,
        }]
    );
}

/// Recorded and scrubbed like the ones above: the list with board fields.
#[test]
fn recorded_search_issues_with_board_fields() {
    type Row<'a> = (u64, Option<(&'a str, usize)>, Option<&'a str>, Vec<&'a str>);
    let issues = search_issues(include_str!(
        "../../fixtures/gh/2.101.0/search-issues-board.json"
    ))
    .unwrap();
    let rows: Vec<Row> = issues
        .iter()
        .map(|i| {
            (
                i.number,
                i.status.as_ref().map(|s| (s.name.as_str(), s.order)),
                i.priority.as_ref().map(|p| p.name.as_str()),
                i.iterations.iter().map(|it| it.title.as_str()).collect(),
            )
        })
        .collect();
    let expected: [Row; 4] = [
        (101, Some(("Todo", 1)), None, vec!["Sprint 40"]),
        (102, Some(("Backlog", 0)), None, vec![]),
        (103, Some(("Backlog", 0)), Some("P1"), vec!["Sprint 41"]),
        (104, Some(("Backlog", 0)), None, vec!["Sprint 40"]),
    ];
    assert_eq!(rows, expected);
    assert_eq!(
        issues[0].labels,
        [("bug", "d73a4a"), ("backend", "7d23f5")].map(|(name, color)| Label {
            name: name.into(),
            color: color.into()
        })
    );
    assert!(
        issues
            .iter()
            .all(|i| i.assignees.is_empty() && i.body.is_empty() && i.comments.is_empty())
    );
}

/// Recorded from gh 2.101.0 on cli/cli (2026-11-18); logins, branches and
/// the comment scrubbed.
#[test]
fn recorded_pull_request_by_number() {
    let pr = pull_request_by_number(include_str!(
        "../../fixtures/gh/2.101.0/pull-request-number.json"
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        (pr.repo.as_str(), pr.number, pr.state),
        ("cli/cli", 15083, PrState::Merged)
    );
    assert_eq!(
        (pr.branch.as_str(), pr.base.as_str()),
        ("feature/event-review", "feature/event-check-suite")
    );
    assert_eq!(
        pr.reviewers,
        ["copilot-pull-request-reviewer", "reviewer-a"]
    );
    assert_eq!(
        pr.comments,
        [Comment {
            author: "author-a".into(),
            body: "Leaving the remaining review threads for a follow-up PR.\n".into(),
            at: 1_794_942_161_000,
        }]
    );
    assert_eq!((pr.checks.len(), pr.rollup), (18, Some(CheckState::Pass)));
    assert_eq!(pr.checks_state(), Some(CheckState::Pass));
}

#[test]
fn nothing_found_by_number() {
    let none = r#"{"data":{"repository":{"issue":null,"pullRequest":null}}}"#;
    assert_eq!(issue(none).unwrap(), None);
    assert_eq!(pull_request_by_number(none).unwrap(), None);
    assert_eq!(issue(r#"{"data":{"repository":null}}"#).unwrap(), None);
}

#[test]
fn recorded_search_issues() {
    let issues =
        search_issues(include_str!("../../fixtures/gh/2.101.0/search-issues.json")).unwrap();
    assert_eq!(issues.len(), 3);
    assert_eq!(
        issues[0],
        Issue {
            repo: "cli/cli".into(),
            number: 14962,
            title: "CLI reports `delete_repo` scope needed on 403 for /dependabot/alerts endpoint"
                .into(),
            url: "https://github.com/cli/cli/issues/14962".into(),
            labels: ["bug", "priority-3", "needs-investigation", "gh-api"]
                .map(label)
                .to_vec(),
            updated_at: 1_794_941_023_000,
            status: None,
            priority: None,
            iterations: vec![],
            assignees: vec![],
            body: String::new(),
            comments: vec![],
        }
    );
    let numbers: Vec<u64> = issues.iter().map(|i| i.number).collect();
    assert_eq!(numbers, [14962, 15066, 15019]);
}

#[test]
fn recorded_search_pull_requests() {
    let json = include_str!("../../fixtures/gh/2.101.0/search-pull-requests.json");
    let prs = search_pull_requests(json).unwrap();
    let summary: Vec<(u64, PrState)> = prs.iter().map(|p| (p.number, p.state)).collect();
    assert_eq!(
        summary,
        [
            (15086, PrState::Open),
            (15084, PrState::Open),
            (14927, PrState::Draft)
        ]
    );
    assert_eq!(prs[2].branch, "feature/multi-account");
    assert!(
        prs.iter()
            .all(|p| p.checks.is_empty() && p.comments.is_empty())
    );
    assert_eq!(
        prs[0].reviewers,
        ["reviewer-a", "copilot-pull-request-reviewer"]
    );
    assert_eq!(search_issues(json).unwrap(), Vec::<Issue>::new());
}

#[test]
fn invalid_json_fails() {
    assert!(pull_request("not json", "b").is_err());
    assert!(search_issues("").is_err());
    assert!(issue("{").is_err());
    assert!(pull_request_by_number("").is_err());
}
