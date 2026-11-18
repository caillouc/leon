//! The `gh` argument lists Leon runs. Each value is one argument; branch,
//! search, repository and comment text go through `-f` (raw: no `@file`
//! reading, no type guessing); `-F` only takes the `{owner}`/`{repo}`
//! placeholders and numbers (sent as GraphQL `Int`).

/// The pull requests of one head branch, with their details; `gh`
/// fills `{owner}` and `{repo}` from the folder it runs in.
const PULL_REQUEST: &str = include_str!("graphql/pull_request.graphql");
/// One pull request by number, with its details.
const PULL_REQUEST_NUMBER: &str = include_str!("graphql/pull_request_number.graphql");
/// One issue by number, with assignees, body and comments.
const ISSUE: &str = include_str!("graphql/issue.graphql");
/// One search over issues and pull requests.
const SEARCH: &str = include_str!("graphql/search.graphql");
/// The pull request fields every query reads.
const PR_FIELDS: &str = include_str!("graphql/pr_fields.graphql");
/// The details of a pull request read alone: body, comments, review
/// threads, checks.
const PR_DETAILS: &str = include_str!("graphql/pr_details.graphql");
/// The issue fields every query reads; `__BOARD__` marks where the board
/// fields go.
const ISSUE_FIELDS: &str = include_str!("graphql/issue_fields.graphql");
/// An issue's fields on its GitHub Project boards; needs the token's
/// `read:project` scope.
const BOARD: &str = include_str!("graphql/board.graphql");

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
        &format!("query={PULL_REQUEST}{PR_FIELDS}{PR_DETAILS}"),
    ])
}

/// `gh api graphql` for pull request `number` of `owner/name`.
pub(crate) fn pull_request_by_number(owner: &str, name: &str, number: u64) -> Vec<String> {
    by_number(
        owner,
        name,
        number,
        &format!("{PULL_REQUEST_NUMBER}{PR_FIELDS}{PR_DETAILS}"),
    )
}

/// `gh api graphql` for issue `number` of `owner/name`; with `board`, with
/// its Project board fields.
pub(crate) fn issue(owner: &str, name: &str, number: u64, board: bool) -> Vec<String> {
    by_number(
        owner,
        name,
        number,
        &format!("{ISSUE}{}", issue_fields(board)),
    )
}

fn by_number(owner: &str, name: &str, number: u64, query: &str) -> Vec<String> {
    owned([
        "api",
        "graphql",
        "-f",
        &format!("owner={owner}"),
        "-f",
        &format!("name={name}"),
        "-F",
        &format!("number={number}"),
        "-f",
        &format!("query={query}"),
    ])
}

/// The owner and the name of an `owner/name` repository.
pub(crate) fn owner_and_name(repo: &str) -> Option<(&str, &str)> {
    let (owner, name) = repo.split_once('/')?;
    let valid = !owner.is_empty() && !name.is_empty() && !name.contains('/');
    valid.then_some((owner, name))
}

/// `gh api graphql` for a GitHub search (`q` in GitHub's search syntax);
/// with `board`, issues come with their Project board fields.
pub(crate) fn search(q: &str, board: bool) -> Vec<String> {
    owned([
        "api",
        "graphql",
        "-f",
        &format!("q={q}"),
        "-f",
        &format!("query={SEARCH}{PR_FIELDS}{}", issue_fields(board)),
    ])
}

/// The issue fields fragment, with the board fields or without them.
fn issue_fields(board: bool) -> String {
    if board {
        format!("{}{BOARD}", ISSUE_FIELDS.replace("__BOARD__", "...board"))
    } else {
        ISSUE_FIELDS.replace("__BOARD__", "")
    }
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

/// `gh pr ready` marking pull request `number` of `repo` ready for review,
/// or with `--undo` converting it to a draft (`gh pr ready --help`, 2.101.0,
/// 2026-11-18).
pub(crate) fn set_draft(repo: &str, number: u64, draft: bool) -> Vec<String> {
    let mut args = owned(["pr", "ready", &number.to_string()]);
    if draft {
        args.push("--undo".to_owned());
    }
    args.extend(owned(["--repo", repo]));
    args
}

/// Answers a review thread (mutation and input from GitHub's GraphQL schema,
/// read 2026-11-18).
const REPLY: &str = "mutation($thread: ID!, $body: String!) { \
    addPullRequestReviewThreadReply(input: {pullRequestReviewThreadId: $thread, body: $body}) \
    { clientMutationId } }";
/// Resolves a review thread (same source).
const RESOLVE: &str = "mutation($thread: ID!) { resolveReviewThread(input: {threadId: $thread}) { clientMutationId } }";

/// `gh api graphql` answering review thread `thread` with `body` (GH-8).
pub(crate) fn reply_to_thread(thread: &str, body: &str) -> Vec<String> {
    owned([
        "api",
        "graphql",
        "-f",
        &format!("thread={thread}"),
        "-f",
        &format!("body={body}"),
        "-f",
        &format!("query={REPLY}"),
    ])
}

/// `gh api graphql` resolving review thread `thread` (GH-8).
pub(crate) fn resolve_thread(thread: &str) -> Vec<String> {
    owned([
        "api",
        "graphql",
        "-f",
        &format!("thread={thread}"),
        "-f",
        &format!("query={RESOLVE}"),
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
    use super::{
        PR_DETAILS, PR_FIELDS, PULL_REQUEST, comment, copilot_review, issue, owner_and_name,
        pull_request, pull_request_by_number, reply_to_thread, resolve_thread, search, set_draft,
    };

    /// The thread id and the text go raw: an `@` or a `=` stays text.
    #[test]
    fn thread_args() {
        let reply = reply_to_thread("PRRT_1", "@me = done");
        assert_eq!(
            reply[..6],
            [
                "api",
                "graphql",
                "-f",
                "thread=PRRT_1",
                "-f",
                "body=@me = done"
            ]
        );
        assert!(reply[7].contains("addPullRequestReviewThreadReply"));
        let resolve = resolve_thread("PRRT_1");
        assert_eq!(resolve[..4], ["api", "graphql", "-f", "thread=PRRT_1"]);
        assert!(resolve[5].contains("resolveReviewThread"));
    }

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
        assert_eq!(
            args[9],
            format!("query={PULL_REQUEST}{PR_FIELDS}{PR_DETAILS}")
        );
        assert_eq!(args.len(), 10);
    }

    /// The repository goes raw, the number typed (`gh` sends `-F` integers
    /// as JSON numbers, which `Int!` needs: live run, gh 2.101.0, 2026-11-18).
    #[test]
    fn by_number_args() {
        for args in [
            pull_request_by_number("@o", "r=1", 12),
            issue("@o", "r=1", 12, true),
        ] {
            assert_eq!(
                args[..8],
                [
                    "api",
                    "graphql",
                    "-f",
                    "owner=@o",
                    "-f",
                    "name=r=1",
                    "-F",
                    "number=12"
                ]
            );
            assert_eq!((args[8].as_str(), args.len()), ("-f", 10));
        }
    }

    #[test]
    fn search_args() {
        assert_eq!(
            search("is:pr author:@me", false)[..4],
            ["api", "graphql", "-f", "q=is:pr author:@me"]
        );
    }

    /// Board fields only when asked: without the `read:project` scope GitHub
    /// refuses the whole query.
    #[test]
    fn board_fields_only_when_asked() {
        let cases = [
            (search("is:issue", true), true),
            (search("is:issue", false), false),
            (issue("o", "r", 1, true), true),
            (issue("o", "r", 1, false), false),
            (pull_request_by_number("o", "r", 1), false),
        ];
        for (args, board) in cases {
            let query = &args[args.len() - 1];
            assert_eq!(query.contains("projectItems"), board, "{query}");
            assert!(!query.contains("__BOARD__"), "{query}");
        }
    }

    /// GitHub rejects a query that spreads a fragment it does not define, or
    /// defines one it does not spread.
    #[test]
    fn queries_carry_exactly_the_fragments_they_use() {
        let queries = [
            pull_request("b"),
            pull_request_by_number("o", "r", 1),
            search("q", false),
            search("q", true),
            issue("o", "r", 1, false),
            issue("o", "r", 1, true),
        ];
        for args in queries {
            let query = args[args.len() - 1].strip_prefix("query=").unwrap();
            let mut spread: Vec<&str> = query
                .split("...")
                .skip(1)
                .filter_map(|s| s.split(|c: char| !c.is_alphanumeric()).next())
                .filter(|name| !name.is_empty() && *name != "on")
                .collect();
            spread.sort_unstable();
            spread.dedup();
            let mut defined: Vec<&str> = query
                .split("fragment ")
                .skip(1)
                .filter_map(|s| s.split_whitespace().next())
                .collect();
            defined.sort_unstable();
            assert_eq!(spread, defined, "{query}");
        }
    }

    #[test]
    fn owner_and_name_of_a_repository() {
        let cases = [
            ("acme/api", Some(("acme", "api"))),
            ("acme", None),
            ("/api", None),
            ("acme/", None),
            ("acme/api/extra", None),
            ("", None),
        ];
        for (repo, expected) in cases {
            assert_eq!(owner_and_name(repo), expected, "{repo}");
        }
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

    /// Never run for real: no test changes a pull request.
    #[test]
    fn set_draft_args() {
        let cases = [
            (true, &["pr", "ready", "12", "--undo", "--repo", "o/r"][..]),
            (false, &["pr", "ready", "12", "--repo", "o/r"][..]),
        ];
        for (draft, expected) in cases {
            assert_eq!(set_draft("o/r", 12, draft), expected, "draft: {draft}");
        }
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
