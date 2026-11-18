//! `gh api graphql` answers into domain types. A node missing a field Leon
//! needs is skipped and unknown fields are ignored; only invalid JSON fails.

use leon_domain::{CheckState, Comment, Issue, Label, PrState, PullRequest};
use serde_json::Value;

use crate::board::{Board, board};
use crate::checks::checks;
use crate::threads::threads;
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

/// One pull request read by number; `None` when GitHub sent none.
pub(crate) fn pull_request_by_number(json: &str) -> Result<Option<PullRequest>, serde_json::Error> {
    in_repository(json, "pullRequest", pull_request_node)
}

/// One issue read by number; `None` when GitHub sent none.
pub(crate) fn issue(json: &str) -> Result<Option<Issue>, serde_json::Error> {
    in_repository(json, "issue", issue_node)
}

fn in_repository<T>(
    json: &str,
    field: &str,
    node: fn(&Value) -> Option<T>,
) -> Result<Option<T>, serde_json::Error> {
    let root: Value = serde_json::from_str(json)?;
    Ok(node(&root["data"]["repository"][field]))
}

/// Issues found by a search.
pub(crate) fn search_issues(json: &str) -> Result<Vec<Issue>, serde_json::Error> {
    search(json, "Issue", issue_node)
}

/// Pull requests found by a search, without body, checks, comments or
/// review threads.
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
        body: text(&pr["body"]).unwrap_or_default(),
        threads: threads(pr),
    })
}

/// An issue; assignees, body and comments are empty when the query did not
/// ask for them (lists).
fn issue_node(issue: &Value) -> Option<Issue> {
    let Board {
        status,
        priority,
        iterations,
    } = board(issue);
    Some(Issue {
        repo: text(&issue["repository"]["nameWithOwner"])?,
        number: issue["number"].as_u64()?,
        title: text(&issue["title"])?,
        url: text(&issue["url"])?,
        labels: labels(issue),
        updated_at: issue["updatedAt"].as_str().and_then(unix_ms).unwrap_or(0),
        status,
        priority,
        iterations,
        assignees: nodes(&issue["assignees"])
            .filter_map(|a| text(&a["login"]))
            .collect(),
        body: text(&issue["body"]).unwrap_or_default(),
        comments: comments(issue),
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

/// Comments of a pull request, an issue or a review thread, oldest first;
/// empty when the query did not ask for them.
pub(crate) fn comments(item: &Value) -> Vec<Comment> {
    nodes(&item["comments"])
        .map(|c| Comment {
            author: c["author"]["login"].as_str().unwrap_or(GHOST).to_owned(),
            body: c["body"].as_str().unwrap_or_default().to_owned(),
            at: c["createdAt"].as_str().and_then(unix_ms).unwrap_or(0),
        })
        .collect()
}

#[cfg(test)]
mod tests;
