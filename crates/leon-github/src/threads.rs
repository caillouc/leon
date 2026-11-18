//! Review threads of a pull request: the ones on its code not resolved yet.

use leon_domain::ReviewThread;
use serde_json::Value;

use crate::parse::{comments, nodes, text};

/// Unresolved review threads of a pull request node, in GitHub's order
/// (oldest first: recorded 2026-11-18); empty when the query did not ask for
/// them (lists).
pub(crate) fn threads(pr: &Value) -> Vec<ReviewThread> {
    nodes(&pr["reviewThreads"]).filter_map(thread).collect()
}

/// `None` for a resolved thread, or one missing its state or its file.
/// `line` is null when GitHub no longer places it (outdated code).
fn thread(node: &Value) -> Option<ReviewThread> {
    if node["isResolved"].as_bool()? {
        return None;
    }
    Some(ReviewThread {
        id: text(&node["id"]).unwrap_or_default(),
        can_resolve: node["viewerCanResolve"].as_bool().unwrap_or(false),
        path: text(&node["path"])?,
        line: node["line"].as_u64().and_then(|l| u32::try_from(l).ok()),
        comments: comments(node),
        code: code(node),
    })
}

/// How many lines of code a thread shows, the commented one last.
const CODE_LINES: usize = 4;

/// The last lines of the first comment's `diffHunk` (its `@@` header left
/// out): GitHub's hunk ends on the commented line (checked on cli/cli,
/// 2026-11-18).
fn code(node: &Value) -> Vec<String> {
    let hunk = nodes(&node["hunk"])
        .find_map(|c| c["diffHunk"].as_str())
        .unwrap_or_default();
    let lines: Vec<&str> = hunk.lines().filter(|l| !l.starts_with("@@")).collect();
    lines[lines.len().saturating_sub(CODE_LINES)..]
        .iter()
        .map(|l| (*l).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use leon_domain::{Comment, ReviewThread};
    use serde_json::{Value, json};

    use super::threads;
    use crate::parse::pull_request_by_number;

    fn comment(author: &str, body: &str, at: i64) -> Comment {
        Comment {
            author: author.into(),
            body: body.into(),
            at,
        }
    }

    #[test]
    fn a_thread_shows_the_last_lines_of_its_code() {
        let hunk = "@@ -10,6 +10,7 @@ fn retry\n fn a() {\n-    old()\n+    new()\n+    more()\n     end()";
        let node = json!({ "isResolved": false, "path": "src/a.rs", "line": 14,
            "comments": { "nodes": [] }, "hunk": { "nodes": [{ "diffHunk": hunk }] } });
        let pr = json!({ "reviewThreads": { "nodes": [node] } });
        assert_eq!(
            threads(&pr)[0].code,
            ["-    old()", "+    new()", "+    more()", "     end()"]
        );
        let without =
            json!({ "reviewThreads": { "nodes": [{ "isResolved": false, "path": "a" }] } });
        assert_eq!(threads(&without)[0].code, Vec::<String>::new());
    }

    #[test]
    fn unresolved_threads_only() {
        let node = |resolved: Value, path: Value, line: Value| {
            json!({ "id": "PRRT_1", "viewerCanResolve": true,
                "isResolved": resolved, "path": path, "line": line, "comments": { "nodes": [
                { "author": { "login": "octocat" }, "body": "Why?", "createdAt": "1970-01-01T00:00:01Z" },
                { "author": null, "body": "Fixed.", "createdAt": "1970-01-01T00:00:02Z" }
            ] } })
        };
        let open = |line| ReviewThread {
            id: "PRRT_1".into(),
            can_resolve: true,
            path: "src/a.rs".into(),
            line,
            comments: vec![
                comment("octocat", "Why?", 1000),
                comment("ghost", "Fixed.", 2000),
            ],
            code: Vec::new(),
        };
        let cases = [
            (
                node(json!(false), json!("src/a.rs"), json!(12)),
                Some(open(Some(12))),
            ),
            (
                node(json!(false), json!("src/a.rs"), Value::Null),
                Some(open(None)),
            ),
            (
                node(json!(false), json!("src/a.rs"), json!(-1)),
                Some(open(None)),
            ),
            (node(json!(true), json!("src/a.rs"), json!(12)), None),
            (node(Value::Null, json!("src/a.rs"), json!(12)), None),
            (node(json!(false), Value::Null, json!(12)), None),
        ];
        for (node, expected) in cases {
            let pr = json!({ "reviewThreads": { "nodes": [node.clone()] } });
            assert_eq!(
                threads(&pr),
                expected.into_iter().collect::<Vec<_>>(),
                "{node}"
            );
        }
    }

    #[test]
    fn none_when_not_asked() {
        assert_eq!(threads(&json!({})), Vec::<ReviewThread>::new());
        assert_eq!(
            threads(&json!({ "reviewThreads": { "nodes": [] } })),
            Vec::<ReviewThread>::new()
        );
    }

    /// Recorded from gh 2.101.0 on cli/cli (2026-11-18) with the whole
    /// by-number query; logins, branches and texts scrubbed. Two of its six
    /// threads are unresolved.
    #[test]
    fn recorded_pull_request() {
        let pr = pull_request_by_number(include_str!(
            "../fixtures/gh/2.101.0/pull-request-threads.json"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(pr.number, 15081);
        assert_eq!(pr.closes, []);
        assert!(pr.body.starts_with("Adds check suite events.\n"));
        let thread = |id: &str, path: &str, line, body: &str| ReviewThread {
            id: id.into(),
            can_resolve: false,
            path: path.into(),
            line: Some(line),
            comments: vec![comment(
                "copilot-pull-request-reviewer",
                body,
                1_794_936_316_000,
            )],
            code: Vec::new(),
        };
        assert_eq!(
            pr.threads,
            [
                thread(
                    "PRRT_kwDODKw3uc64kEVt",
                    "pkg/cmd/event/shared/hint.go",
                    35,
                    "This hint is printed even when the list is empty."
                ),
                thread(
                    "PRRT_kwDODKw3uc64kEWr",
                    "pkg/cmd/run/view/hint.go",
                    15,
                    "The flag name in this message does not match the command."
                ),
            ]
        );
    }
}
