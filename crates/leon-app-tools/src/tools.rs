//! The five tools: what `tools/list` shows agents, and how `tools/call`
//! arguments become an [`AppToolCall`].

use std::path::PathBuf;

use leon_core::ports::AppToolCall;
use serde_json::{Value, json};

const START_AGENT: &str = "start_agent";
const MESSAGE_AGENT: &str = "message_agent";
const RENAME_FLOW: &str = "rename_flow";
const SET_WORKING_DIRECTORY: &str = "set_working_directory";
const LINK_GITHUB: &str = "link_github";

/// What every conversation is told (CTX-3): Leon shows the user the
/// agents' work only as well as they keep it up to date.
pub(crate) const INSTRUCTIONS: &str = "You run inside Leon, an app that shows the user \
what every agent of this flow is doing. Keep it up to date with the tools of its \
`leon` MCP server, each time it applies, without being asked:
- Each time you start working in another folder (a git worktree you create or enter, \
another repository), call `set_working_directory` with its absolute path, before you work \
there.
- Once the subject of the flow is clear, and when it changes, call `rename_flow` \
with a short title.
- Each time you create a GitHub issue or pull request, or start working on one, call \
`link_github` with its link.
- When the user asks you to start an agent for them (they will review and continue it \
themselves), call `start_agent` with `for_user: true`: its answers go to the user, not \
back to you.
- Leon names this flow's conversations for the user as `[From …]` messages show them \
(`planner`, `coder 1`, `coder 2`…): when the user names one, pass that name to \
`message_agent`.";

/// Why a `tools/call` could not become an [`AppToolCall`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ToolError {
    /// No tool has this name.
    Unknown,
    /// The arguments are wrong; the message is for the agent.
    Arguments(String),
}

/// The `tools/list` result.
pub(crate) fn list() -> Value {
    json!({ "tools": [
        start_agent_tool(),
        {
            "name": MESSAGE_AGENT,
            "description": "Send a follow-up message to another conversation of this flow. \
                Returns at once: do not wait for it. When that agent finishes its answer, the \
                answer arrives in this conversation as a new message starting with `[From \
                <name> · conversation <id>]`.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "conversation": {
                        "type": "string",
                        "description": "The conversation: its name as Leon shows it to the \
                            user (`coder 2`; a name gains a number when another conversation \
                            of the same agent starts), or its id (`14`), as start_agent \
                            returned them or a `[From <name> · conversation <id>]` message \
                            shows them.",
                    },
                    "text": { "type": "string", "description": "The message." },
                },
                "required": ["conversation", "text"],
            },
        },
        {
            "name": RENAME_FLOW,
            "description": "Rename this flow in Leon's list of flows. Use a short title that \
                says what the work is about.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "The new title, a few words." },
                },
                "required": ["title"],
            },
        },
        {
            "name": SET_WORKING_DIRECTORY,
            "description": "Tell Leon this conversation now also works in another folder: a \
                git worktree you created or entered, or another repository. Leon then shows \
                that folder's changes and pull request with the flow's other folders. Call it \
                each time you start working in a new folder, before you work there.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path of the folder." },
                },
                "required": ["path"],
            },
        },
        {
            "name": LINK_GITHUB,
            "description": "Link a GitHub issue or pull request to this flow: Leon then shows \
                it in the flow's Info, with its state, labels and checks. Call it each time \
                you create an issue or a pull request, or start working on one.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "Its link: https://github.com/<owner>/<repo>/issues/<n> \
                            or …/pull/<n>.",
                    },
                },
                "required": ["url"],
            },
        },
    ] })
}

/// `start_agent` in `tools/list`.
fn start_agent_tool() -> Value {
    json!({
        "name": START_AGENT,
        "description": "Start another agent in a new conversation of this flow and send it \
            a first message. Returns at once with the new conversation's name and id: do \
            not wait for it. When that agent finishes its answer, the answer arrives in \
            this conversation as a new message starting with `[From <name> · \
            conversation <id>]`. Meanwhile, keep working or end your turn. Send it \
            follow-ups with message_agent.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "Name of the agent to start, as in its agent file.",
                },
                "prompt": {
                    "type": "string",
                    "description": "First message to the agent. It starts a new \
                        conversation and does not see this one, so give it everything it \
                        needs.",
                },
                "working_directory": {
                    "type": "string",
                    "description": "Absolute path of the folder it works in, e.g. a git \
                        worktree. Defaults to this conversation's working directory.",
                },
                "for_user": {
                    "type": "boolean",
                    "description": "true when the user will review and continue this \
                        conversation themselves: its answers go to the user, never back \
                        to you. Defaults to false: its answer comes back to you.",
                },
            },
            "required": ["agent", "prompt"],
        },
    })
}

/// Turns a `tools/call`'s `name` and `arguments` into a call to the core.
pub(crate) fn parse(name: &str, arguments: &Value) -> Result<AppToolCall, ToolError> {
    Ok(match name {
        START_AGENT => AppToolCall::StartAgent {
            agent: text(arguments, "agent")?,
            prompt: text(arguments, "prompt")?,
            working_directory: arguments
                .get("working_directory")
                .filter(|value| !value.is_null() && *value != "")
                .map(|_| path(arguments, "working_directory"))
                .transpose()?,
            for_user: arguments
                .get("for_user")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        MESSAGE_AGENT => AppToolCall::MessageAgent {
            // An id given as a number is taken too.
            conversation: match arguments.get("conversation").and_then(Value::as_i64) {
                Some(id) => id.to_string(),
                None => text(arguments, "conversation")?,
            },
            text: text(arguments, "text")?,
        },
        RENAME_FLOW => AppToolCall::RenameFlow {
            title: text(arguments, "title")?,
        },
        SET_WORKING_DIRECTORY => AppToolCall::SetWorkingDirectory {
            path: path(arguments, "path")?,
        },
        LINK_GITHUB => AppToolCall::LinkGithub {
            url: text(arguments, "url")?,
        },
        _ => return Err(ToolError::Unknown),
    })
}

/// A required, non-blank string argument.
fn text(arguments: &Value, key: &str) -> Result<String, ToolError> {
    match arguments.get(key).and_then(Value::as_str) {
        Some(value) if !value.trim().is_empty() => Ok(value.to_owned()),
        _ => Err(ToolError::Arguments(format!(
            "`{key}` is required: a non-empty string"
        ))),
    }
}

/// A required absolute path argument.
fn path(arguments: &Value, key: &str) -> Result<PathBuf, ToolError> {
    let path = PathBuf::from(text(arguments, key)?);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(ToolError::Arguments(format!(
            "`{key}` must be an absolute path, got {}",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{ToolError, list, parse};

    #[test]
    fn every_listed_tool_parses_its_required_arguments() {
        let samples = json!({
            "agent": "coder", "prompt": "do it", "conversation": 3, "text": "more",
            "title": "Title", "path": "/repo", "url": "https://github.com/acme/api/issues/1",
        });
        let tools = list()["tools"].as_array().cloned().unwrap_or_default();
        assert_eq!(tools.len(), 5);
        for tool in tools {
            let name = tool["name"].as_str().unwrap_or_default();
            let mut arguments = json!({});
            for key in tool["inputSchema"]["required"].as_array().unwrap() {
                let key = key.as_str().unwrap();
                arguments[key] = samples[key].clone();
            }
            assert!(parse(name, &arguments).is_ok(), "{name} with {arguments}");
        }
    }

    #[test]
    fn bad_arguments_say_what_is_wrong() {
        let cases: &[(&str, Value, &str)] = &[
            (
                "start_agent",
                json!({ "prompt": "p" }),
                "`agent` is required",
            ),
            (
                "start_agent",
                json!({ "agent": " ", "prompt": "p" }),
                "`agent` is required",
            ),
            (
                "start_agent",
                json!({ "agent": "a", "prompt": 3 }),
                "`prompt` is required",
            ),
            (
                "start_agent",
                json!({ "agent": "a", "prompt": "p", "working_directory": "rel" }),
                "`working_directory` must be an absolute path",
            ),
            (
                "message_agent",
                json!({ "conversation": " ", "text": "t" }),
                "`conversation` is required",
            ),
            ("link_github", json!({}), "`url` is required"),
            (
                "message_agent",
                json!({ "conversation": 3 }),
                "`text` is required",
            ),
            ("rename_flow", json!("title"), "`title` is required"),
            ("rename_flow", Value::Null, "`title` is required"),
            (
                "set_working_directory",
                json!({ "path": "src" }),
                "`path` must be an absolute path",
            ),
        ];
        for (name, arguments, expected) in cases {
            match parse(name, arguments) {
                Err(ToolError::Arguments(message)) => {
                    assert!(message.contains(expected), "{name} {arguments}: {message}");
                }
                other => panic!("{name} {arguments}: {other:?}"),
            }
        }
    }

    #[test]
    fn null_or_empty_working_directory_means_none() {
        for directory in [Value::Null, json!("")] {
            assert_eq!(
                parse(
                    "start_agent",
                    &json!({ "agent": "a", "prompt": "p", "working_directory": directory })
                ),
                Ok(leon_core::ports::AppToolCall::StartAgent {
                    agent: "a".to_owned(),
                    prompt: "p".to_owned(),
                    working_directory: None,
                    for_user: false,
                })
            );
        }
    }

    #[test]
    fn unknown_tool() {
        assert_eq!(
            parse("delete_everything", &json!({})),
            Err(ToolError::Unknown)
        );
    }
}
