//! The `gh` argument lists Leon runs. Each value is one argument; branch,
//! search and comment text go through `-f` (raw: no `@file` reading, no
//! type guessing), only the `{owner}`/`{repo}` placeholders through `-F`.

/// The pull requests of one head branch, with comments and checks; `gh`
/// fills `{owner}` and `{repo}` from the folder it runs in.
const PULL_REQUEST: &str = include_str!("graphql/pull_request.graphql");
/// One search over issues and pull requests.
const SEARCH: &str = include_str!("graphql/search.graphql");
/// The pull request fields both queries read.
const PR_FIELDS: &str = include_str!("graphql/pr_fields.graphql");

/// Open issues assigned to the user (GH-2).
pub(crate) const MY_ISSUES: &str = "is:issue is:open assignee:@me sort:updated-desc";
/// Open pull requests the user opened (GH-3).
pub(crate) const MY_PULL_REQUESTS: &str = "is:pr is:open author:@me sort:updated-desc";

fn owned<const N: usize>(args: [&str; N]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

/// `gh api graphql` for the pull requests whose head is `branch`, run in the
/// repository's folder.
pub(crate) fn pull_request(branch: &str) -> Vec<String> {
    owned([
        "api",
        "graphql",
        "-F",
        "owner={owner}",
        "-F",
        "name={repo}",
        "-f",
        &format!("branch={branch}"),
        "-f",
        &format!("query={PULL_REQUEST}{PR_FIELDS}"),
    ])
}

/// `gh api graphql` for a GitHub search (`q` in GitHub's search syntax).
pub(crate) fn search(q: &str) -> Vec<String> {
    owned([
        "api",
        "graphql",
        "-f",
        &format!("q={q}"),
        "-f",
        &format!("query={SEARCH}{PR_FIELDS}"),
    ])
}

/// `gh pr edit` asking Copilot to review pull request `number` of `repo`.
pub(crate) fn copilot_review(repo: &str, number: u64) -> Vec<String> {
    owned([
        "pr",
        "edit",
        &number.to_string(),
        "--repo",
        repo,
        "--add-reviewer",
        "@copilot",
    ])
}

/// `gh api` posting `body` as a comment on issue or pull request `number`
/// of `repo` (both are issues to the REST API).
pub(crate) fn comment(repo: &str, number: u64, body: &str) -> Vec<String> {
    owned([
        "api",
        "-X",
        "POST",
        &format!("repos/{repo}/issues/{number}/comments"),
        "-f",
        &format!("body={body}"),
    ])
}

#[cfg(test)]
mod tests {
    use super::{PR_FIELDS, PULL_REQUEST, SEARCH, comment, copilot_review, pull_request, search};

    #[test]
    fn pull_request_passes_the_branch_raw() {
        let args = pull_request("@feature/x=1");
        assert_eq!(
            args[..8],
            [
                "api",
                "graphql",
                "-F",
                "owner={owner}",
                "-F",
                "name={repo}",
                "-f",
                "branch=@feature/x=1"
            ]
        );
        assert_eq!(args[8], "-f");
        assert_eq!(args[9], format!("query={PULL_REQUEST}{PR_FIELDS}"));
        assert_eq!(args.len(), 10);
    }

    #[test]
    fn search_args() {
        let args = search("is:pr author:@me");
        assert_eq!(args[..4], ["api", "graphql", "-f", "q=is:pr author:@me"]);
        assert_eq!(
            args[4..],
            ["-f".to_owned(), format!("query={SEARCH}{PR_FIELDS}")]
        );
    }

    /// The queries spread `...pr`; every one must carry its definition.
    #[test]
    fn queries_carry_the_fragment_they_use() {
        assert!(PR_FIELDS.starts_with("fragment pr on PullRequest"));
        assert!(PULL_REQUEST.contains("...pr ") && SEARCH.contains("...pr\n"));
    }

    #[test]
    fn copilot_review_args() {
        assert_eq!(
            copilot_review("o/r", 12),
            [
                "pr",
                "edit",
                "12",
                "--repo",
                "o/r",
                "--add-reviewer",
                "@copilot"
            ]
        );
    }

    #[test]
    fn comment_body_stays_one_raw_argument() {
        let body = "Looks good; `$(rm -rf ~)` \"quoted\"\n@file --flag";
        assert_eq!(
            comment("o/r", 7, body),
            [
                "api".to_owned(),
                "-X".to_owned(),
                "POST".to_owned(),
                "repos/o/r/issues/7/comments".to_owned(),
                "-f".to_owned(),
                format!("body={body}"),
            ]
        );
    }
}
