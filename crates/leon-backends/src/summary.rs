//! One-line descriptions of tool calls, shared by every CLI.

use serde_json::Value;

use crate::process::clip;

/// Most characters of a tool's output kept in a transcript.
pub(crate) const OUTPUT_MAX: usize = 8_000;

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
        other => return (other.to_owned(), compact(input)),
    };
    (verb.to_owned(), clip(&detail, 200))
}

/// Short JSON for a tool input with no known shape.
pub(crate) fn compact(input: &Value) -> String {
    if input.is_null() || input.as_object().is_some_and(serde_json::Map::is_empty) {
        return String::new();
    }
    clip(&input.to_string(), 160)
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

    use super::{claude_tool, result_text};

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
        ];
        for (name, input, verb, detail) in cases {
            assert_eq!(
                claude_tool(name, &input),
                (verb.to_owned(), detail.to_owned()),
                "{name}"
            );
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
