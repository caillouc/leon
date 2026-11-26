//! One-line descriptions of tool calls, shared by every CLI.

use std::path::PathBuf;

use leon_core::ports::BackendEvent;
use serde_json::Value;

use crate::process::clip;

/// Most characters of a tool's output kept in a transcript.
pub(crate) const OUTPUT_MAX: usize = 8_000;

/// Most of a tool call's detail kept (its command, its input), shown whole
/// when the user unfolds the call or is asked to allow it: a long command
/// whole, a script cut.
pub(crate) const DETAIL_MAX: usize = 2_000;

/// Verb and detail for a Claude Code tool call, e.g. `("Ran", "cargo test")`.
pub(crate) fn claude_tool(name: &str, input: &Value) -> (String, String) {
    let field = |key: &str| {
        input
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let (verb, detail) = match name {
        "Bash" => ("Ran", field("command")),
        "Read" => ("Read", field("file_path")),
        "Edit" | "MultiEdit" => ("Edited", field("file_path")),
        "Write" => ("Wrote", field("file_path")),
        "Grep" => ("Searched", field("pattern")),
        "Glob" => ("Listed", field("pattern")),
        "WebFetch" => ("Fetched", field("url")),
        "WebSearch" => ("Searched the web", field("query")),
        "Task" | "Agent" => ("Started a subagent", field("description")),
        "TodoWrite" => ("Updated the task list", String::new()),
        // Its questions, not its JSON: the answer follows (CONV-9).
        "AskUserQuestion" => (
            "Asked",
            input["questions"].as_array().map_or_default(|qs| {
                qs.iter()
                    .filter_map(|q| q["question"].as_str())
                    .collect::<Vec<_>>()
                    .join(" · ")
            }),
        ),
        other => return (other.to_owned(), compact(input)),
    };
    (verb.to_owned(), clip(&detail, DETAIL_MAX))
}

/// What a Claude Code tool call is for, as the agent said it: a command's
/// `description` (Claude Code 2.1.288's Bash input, seen 2026-11-26).
pub(crate) fn claude_purpose(name: &str, input: &Value) -> Option<String> {
    (name == "Bash")
        .then(|| input["description"].as_str())
        .flatten()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| clip(d, DETAIL_MAX))
}

/// The files a tool call changes, as an event; `None` without any (CTX-2).
pub(crate) fn files_changed<'a>(
    paths: impl IntoIterator<Item = &'a Value>,
) -> Option<BackendEvent> {
    let mut files: Vec<PathBuf> = paths
        .into_iter()
        .filter_map(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .collect();
    files.dedup();
    (!files.is_empty()).then_some(BackendEvent::FilesChanged(files))
}

/// The file a Claude Code tool call changes, if it changes one.
pub(crate) fn claude_files(name: &str, input: &Value) -> Option<BackendEvent> {
    let key = match name {
        "Edit" | "MultiEdit" | "Write" => "file_path",
        "NotebookEdit" => "notebook_path",
        _ => return None,
    };
    files_changed([&input[key]])
}

/// One-line JSON for a tool input with no known shape.
pub(crate) fn compact(input: &Value) -> String {
    if input.is_null() || input.as_object().is_some_and(serde_json::Map::is_empty) {
        return String::new();
    }
    clip(&input.to_string(), DETAIL_MAX)
}

/// Text of a tool result: a string or a list of text blocks.
pub(crate) fn result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use std::path::PathBuf;

    use leon_core::ports::BackendEvent;

    use super::{claude_files, claude_tool, result_text};

    #[test]
    fn tools_that_change_files_name_them() {
        let changed = |paths: &[&str]| {
            Some(BackendEvent::FilesChanged(
                paths.iter().map(PathBuf::from).collect(),
            ))
        };
        let cases = [
            (
                "Edit",
                json!({"file_path": "/r/a.rs"}),
                changed(&["/r/a.rs"]),
            ),
            (
                "Write",
                json!({"file_path": "/r/b.rs"}),
                changed(&["/r/b.rs"]),
            ),
            (
                "NotebookEdit",
                json!({"notebook_path": "/r/n.ipynb"}),
                changed(&["/r/n.ipynb"]),
            ),
            ("Read", json!({"file_path": "/r/a.rs"}), None),
            ("Edit", json!({}), None),
        ];
        for (name, input, expected) in cases {
            assert_eq!(claude_files(name, &input), expected, "{name}");
        }
    }

    #[test]
    fn known_tools_get_a_verb() {
        let cases = [
            (
                "Bash",
                json!({"command": "cargo test"}),
                "Ran",
                "cargo test",
            ),
            ("Read", json!({"file_path": "/a.rs"}), "Read", "/a.rs"),
            (
                "Edit",
                json!({"file_path": "/a.rs", "old_string": "x"}),
                "Edited",
                "/a.rs",
            ),
            ("Grep", json!({"pattern": "foo"}), "Searched", "foo"),
            (
                "AskUserQuestion",
                json!({"questions": [{"question": "Base?", "options": []}, {"question": "Commit?"}]}),
                "Asked",
                "Base? · Commit?",
            ),
        ];
        for (name, input, verb, detail) in cases {
            assert_eq!(
                claude_tool(name, &input),
                (verb.to_owned(), detail.to_owned()),
                "{name}"
            );
        }
    }

    /// Unfolded in the transcript, a long command shows whole (CONV-20).
    #[test]
    fn a_long_command_is_kept_whole() {
        let command = format!("gcloud redis instances list {}", "--region x ".repeat(40));
        let (_, detail) = claude_tool("Bash", &json!({ "command": command }));
        assert_eq!(detail, command);
    }

    #[test]
    fn a_command_says_what_it_is_for() {
        use super::claude_purpose;
        let cases = [
            (
                "Bash",
                json!({"command": "ls", "description": "List files"}),
                Some("List files"),
            ),
            ("Bash", json!({"command": "ls", "description": " "}), None),
            ("Bash", json!({"command": "ls"}), None),
            ("Read", json!({"file_path": "/a", "description": "x"}), None),
        ];
        for (name, input, expected) in cases {
            assert_eq!(claude_purpose(name, &input).as_deref(), expected, "{input}");
        }
    }

    #[test]
    fn unknown_tools_keep_their_name() {
        let (verb, detail) = claude_tool("mcp__slack__read", &json!({"channel": "c1"}));
        assert_eq!(verb, "mcp__slack__read");
        assert_eq!(detail, r#"{"channel":"c1"}"#);
    }

    #[test]
    fn result_text_accepts_both_shapes() {
        assert_eq!(result_text(&json!("plain")), "plain");
        assert_eq!(
            result_text(&json!([{"type": "text", "text": "a"}, {"type": "text", "text": "b"}])),
            "a\nb"
        );
    }
}
