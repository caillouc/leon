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
        path: text(&node["path"])?,
        line: node["line"].as_u64().and_then(|l| u32::try_from(l).ok()),
        comments: comments(node),
    })
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
    fn unresolved_threads_only() {
        let node = |resolved: Value, path: Value, line: Value| {
            json!({ "isResolved": resolved, "path": path, "line": line, "comments": { "nodes": [
                { "author": { "login": "octocat" }, "body": "Why?", "createdAt": "1970-01-01T00:00:01Z" },
                { "author": null, "body": "Fixed.", "createdAt": "1970-01-01T00:00:02Z" }
            ] } })
        };
        let open = |line| ReviewThread {
            path: "src/a.rs".into(),
            line,
            comments: vec![
                comment("octocat", "Why?", 1000),
                comment("ghost", "Fixed.", 2000),
            ],
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
        assert!(pr.body.starts_with("Adds check suite events.\n"));
        let thread = |path: &str, line, body: &str| ReviewThread {
            path: path.into(),
            line: Some(line),
            comments: vec![comment(
                "copilot-pull-request-reviewer",
                body,
                1_794_936_316_000,
            )],
        };
        assert_eq!(
            pr.threads,
            [
                thread(
                    "pkg/cmd/event/shared/hint.go",
                    35,
                    "This hint is printed even when the list is empty."
                ),
                thread(
                    "pkg/cmd/run/view/hint.go",
                    15,
                    "The flag name in this message does not match the command."
                ),
            ]
        );
    }
}
