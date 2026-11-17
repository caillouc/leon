//! `gh api graphql` answers into domain types. A node missing a field Leon
//! needs is skipped and unknown fields are ignored; only invalid JSON fails.

use leon_domain::{CheckState, Comment, Issue, IssueStatus, Label, PrState, PullRequest};
use serde_json::Value;

use crate::checks::checks;
use crate::time::unix_ms;

/// Login GitHub shows for a deleted account.
const GHOST: &str = "ghost";

/// The pull request of `branch`: the newest open one, else the newest one.
/// `None` without any, or when `branch` is the repository's default branch
/// (forks' pull requests from their own default branch share its name).
pub(crate) fn pull_request(
    json: &str,
    branch: &str,
) -> Result<Option<PullRequest>, serde_json::Error> {
    let root: Value = serde_json::from_str(json)?;
    let repository = &root["data"]["repository"];
    if repository["defaultBranchRef"]["name"].as_str() == Some(branch) {
        return Ok(None);
    }
    let mut prs: Vec<PullRequest> = nodes(&repository["pullRequests"])
        .filter_map(pull_request_node)
        .collect();
    let open = prs
        .iter()
        .position(|pr| matches!(pr.state, PrState::Open | PrState::Draft));
    Ok(match open {
        Some(i) => Some(prs.swap_remove(i)),
        None if prs.is_empty() => None,
        None => Some(prs.swap_remove(0)),
    })
}

/// Issues found by a search.
pub(crate) fn search_issues(json: &str) -> Result<Vec<Issue>, serde_json::Error> {
    search(json, "Issue", issue_node)
}

/// Pull requests found by a search, without checks or comments.
pub(crate) fn search_pull_requests(json: &str) -> Result<Vec<PullRequest>, serde_json::Error> {
    search(json, "PullRequest", pull_request_node)
}

fn search<T>(
    json: &str,
    typename: &str,
    node: fn(&Value) -> Option<T>,
) -> Result<Vec<T>, serde_json::Error> {
    let root: Value = serde_json::from_str(json)?;
    Ok(nodes(&root["data"]["search"])
        .filter(|n| n["__typename"] == typename)
        .filter_map(node)
        .collect())
}

/// The `nodes` of a GraphQL connection; none when absent.
pub(crate) fn nodes(connection: &Value) -> impl Iterator<Item = &Value> {
    connection["nodes"].as_array().into_iter().flatten()
}

/// A string field, owned.
pub(crate) fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

fn pull_request_node(pr: &Value) -> Option<PullRequest> {
    Some(PullRequest {
        repo: text(&pr["repository"]["nameWithOwner"])?,
        number: pr["number"].as_u64()?,
        title: text(&pr["title"])?,
        url: text(&pr["url"])?,
        state: pr_state(pr)?,
        branch: text(&pr["headRefName"])?,
        base: text(&pr["baseRefName"])?,
        labels: labels(pr),
        checks: checks(pr),
        rollup: rollup(pr),
        reviewers: reviewers(pr),
        comments: comments(pr),
    })
}

fn issue_node(issue: &Value) -> Option<Issue> {
    Some(Issue {
        repo: text(&issue["repository"]["nameWithOwner"])?,
        number: issue["number"].as_u64()?,
        title: text(&issue["title"])?,
        url: text(&issue["url"])?,
        labels: labels(issue),
        updated_at: issue["updatedAt"].as_str().and_then(unix_ms).unwrap_or(0),
        status: status(issue),
    })
}

fn pr_state(pr: &Value) -> Option<PrState> {
    let draft = pr["isDraft"].as_bool().unwrap_or(false);
    Some(match pr["state"].as_str()? {
        "OPEN" if draft => PrState::Draft,
        "OPEN" => PrState::Open,
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ => return None,
    })
}

fn labels(item: &Value) -> Vec<Label> {
    nodes(&item["labels"])
        .filter_map(|l| {
            Some(Label {
                name: text(&l["name"])?,
                color: text(&l["color"]).unwrap_or_default(),
            })
        })
        .collect()
}

/// GitHub's summary of the last commit's checks: `SUCCESS`, `FAILURE`,
/// `ERROR`, `PENDING`, `EXPECTED`.
fn rollup(pr: &Value) -> Option<CheckState> {
    match pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["state"].as_str()? {
        "SUCCESS" => Some(CheckState::Pass),
        "FAILURE" | "ERROR" => Some(CheckState::Fail),
        _ => Some(CheckState::Pending),
    }
}

/// The issue's "Status" on the first Project board that has one; its order
/// is the column's place on that board.
fn status(issue: &Value) -> Option<IssueStatus> {
    nodes(&issue["projectItems"]).find_map(|item| {
        let value = &item["fieldValueByName"];
        let name = text(&value["name"])?;
        let option = value["optionId"].as_str();
        let order = nodes_array(&value["field"]["options"])
            .position(|o| o["id"].as_str() == option)
            .unwrap_or(usize::MAX);
        Some(IssueStatus {
            name,
            order,
            color: text(&value["color"]),
        })
    })
}

/// The items of a plain JSON array; none when absent.
fn nodes_array(array: &Value) -> impl Iterator<Item = &Value> {
    array.as_array().into_iter().flatten()
}

/// Requested reviewers (users, bots, `org/team`), then past reviewers, each
/// login once.
fn reviewers(pr: &Value) -> Vec<String> {
    let requested = nodes(&pr["reviewRequests"]).filter_map(|r| {
        let reviewer = &r["requestedReviewer"];
        reviewer["login"]
            .as_str()
            .or_else(|| reviewer["combinedSlug"].as_str())
    });
    let reviewed = nodes(&pr["latestReviews"]).filter_map(|r| r["author"]["login"].as_str());
    let mut logins: Vec<String> = Vec::new();
    for login in requested.chain(reviewed) {
        if !logins.iter().any(|l| l == login) {
            logins.push(login.to_owned());
        }
    }
    logins
}

/// Comments, oldest first; empty when the query did not ask for them.
fn comments(pr: &Value) -> Vec<Comment> {
    nodes(&pr["comments"])
        .map(|c| Comment {
            author: c["author"]["login"].as_str().unwrap_or(GHOST).to_owned(),
            body: c["body"].as_str().unwrap_or_default().to_owned(),
            at: c["createdAt"].as_str().and_then(unix_ms).unwrap_or(0),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use leon_domain::{
        Check, CheckState, Comment, Issue, IssueStatus, Label, PrState, PullRequest,
    };
    use serde_json::json;

    use super::{pull_request, search_issues, search_pull_requests};

    fn fixture_pr(json: &str, branch: &str) -> PullRequest {
        pull_request(json, branch).unwrap().unwrap()
    }

    /// Recorded from gh 2.101.0 on cli/cli (2026-11-17), logins scrubbed.
    #[test]
    fn recorded_open_pull_request() {
        let branch = "dependabot/npm_and_yarn/evals/zod-4.6.6";
        let pr = fixture_pr(
            include_str!("../fixtures/gh/2.101.0/pull-request-open.json"),
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
            include_str!("../fixtures/gh/2.101.0/pull-request-merged.json"),
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
        let json = include_str!("../fixtures/gh/2.101.0/pull-request-none.json");
        assert_eq!(
            pull_request(json, "leon-fixture-no-such-branch").unwrap(),
            None
        );
    }

    /// cli/cli's default branch `trunk` matches pull requests from forks.
    #[test]
    fn default_branch_has_no_pull_request() {
        let json = include_str!("../fixtures/gh/2.101.0/pull-request-default-branch.json");
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

    /// The shape of a live `gh api graphql` answer (2026-11-17) with the
    /// project status, label colors and check summary; values made up.
    #[test]
    fn status_colors_and_check_summary() {
        let json = r#"{"data":{"search":{"nodes":[
          {"__typename":"Issue","number":7,"title":"Retry","url":"https://github.com/o/r/issues/7",
           "updatedAt":"2026-11-17T10:00:00Z","repository":{"nameWithOwner":"o/r"},
           "labels":{"nodes":[{"name":"bug","color":"d73a4a"}]},
           "projectItems":{"nodes":[{"fieldValueByName":null},
             {"fieldValueByName":{"name":"In progress","color":"YELLOW","optionId":"b",
               "field":{"options":[{"id":"a"},{"id":"b"},{"id":"c"}]}}}]}},
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
        assert_eq!(
            issue.status,
            Some(IssueStatus {
                name: "In progress".into(),
                order: 1,
                color: Some("YELLOW".into()),
            })
        );
        let pr = &search_pull_requests(json).unwrap()[0];
        assert_eq!(pr.checks_state(), Some(CheckState::Fail));
    }

    #[test]
    fn recorded_search_issues() {
        let issues =
            search_issues(include_str!("../fixtures/gh/2.101.0/search-issues.json")).unwrap();
        assert_eq!(issues.len(), 3);
        assert_eq!(
            issues[0],
            Issue {
                repo: "cli/cli".into(),
                number: 14962,
                title:
                    "CLI reports `delete_repo` scope needed on 403 for /dependabot/alerts endpoint"
                        .into(),
                url: "https://github.com/cli/cli/issues/14962".into(),
                labels: ["bug", "priority-3", "needs-investigation", "gh-api"]
                    .map(label)
                    .to_vec(),
                updated_at: 1_794_941_023_000,
                status: None,
            }
        );
        let numbers: Vec<u64> = issues.iter().map(|i| i.number).collect();
        assert_eq!(numbers, [14962, 15066, 15019]);
    }

    #[test]
    fn recorded_search_pull_requests() {
        let json = include_str!("../fixtures/gh/2.101.0/search-pull-requests.json");
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
    }
}
