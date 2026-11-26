//! `session/update` notifications of a turn turned into Leon events.

use std::collections::HashMap;

use leon_core::ports::BackendEvent;
use leon_domain::Usage;
use serde_json::Value;

use crate::process::clip;
use crate::summary::{DETAIL_MAX, OUTPUT_MAX, files_changed, result_text};

/// A tool call that has not finished.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct OpenTool {
    pub title: String,
    pub kind: String,
    /// The shell command it runs (`rawInput.command`), when it runs one: what
    /// a permission request about it shows (Copilot CLI 1.0.114 asks with the
    /// paths only, under the same id).
    pub command: Option<String>,
}

/// The agent message being streamed and the tool calls still open.
#[derive(Debug, Default)]
pub(super) struct Turn {
    message: Option<String>,
    text: String,
    tools: HashMap<String, OpenTool>,
}

impl Turn {
    /// One `session/update`. Thoughts, plans, usage, commands and mode
    /// changes are not shown.
    pub(super) fn update(&mut self, update: &Value) -> Vec<BackendEvent> {
        match update["sessionUpdate"].as_str() {
            Some("agent_message_chunk") => self.chunk(update),
            Some("tool_call") => self.tool_call(update),
            Some("tool_call_update") => self.tool_update(update),
            _ => Vec::new(),
        }
    }

    /// The `session/prompt` response: the last message, then the end.
    pub(super) fn end(&mut self, result: &Value) -> Vec<BackendEvent> {
        let mut out = self.close();
        out.push(BackendEvent::TurnEnded {
            stopped: result["stopReason"] == "cancelled",
            usage: usage(&result["usage"]),
        });
        out
    }

    /// The message streamed so far, as a whole; forgets the turn.
    pub(super) fn close(&mut self) -> Vec<BackendEvent> {
        self.tools.clear();
        self.message = None;
        self.flush().into_iter().collect()
    }

    /// An open tool call, for permission requests that only give its id.
    pub(super) fn tool(&self, id: &str) -> Option<&OpenTool> {
        self.tools.get(id)
    }

    fn chunk(&mut self, update: &Value) -> Vec<BackendEvent> {
        let Some(text) = update["content"]["text"]
            .as_str()
            .filter(|_| update["content"]["type"] == "text")
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // A new `messageId` starts a new message.
        if let Some(id) = update["messageId"].as_str()
            && self.message.as_deref() != Some(id)
        {
            out.extend(self.flush());
            self.message = Some(id.to_owned());
        }
        self.text.push_str(text);
        out.push(BackendEvent::TextDelta(text.to_owned()));
        out
    }

    fn flush(&mut self) -> Option<BackendEvent> {
        let text = std::mem::take(&mut self.text);
        (!text.trim().is_empty()).then_some(BackendEvent::Text(text))
    }

    fn tool_call(&mut self, update: &Value) -> Vec<BackendEvent> {
        let Some(id) = update["toolCallId"].as_str() else {
            return Vec::new();
        };
        // The message before a tool call is complete.
        let mut out: Vec<BackendEvent> = self.flush().into_iter().collect();
        let tool = OpenTool {
            title: update["title"].as_str().unwrap_or_default().to_owned(),
            kind: update["kind"].as_str().unwrap_or("other").to_owned(),
            command: command(update),
        };
        let (name, detail, purpose) = describe(&tool);
        out.push(BackendEvent::Tool {
            id: id.to_owned(),
            name,
            detail,
            purpose,
        });
        self.tools.insert(id.to_owned(), tool);
        out.extend(self.tool_update(update));
        out
    }

    /// An update of a tool call: its command if it gives one, the files an
    /// edit names, then its end.
    fn tool_update(&mut self, update: &Value) -> Vec<BackendEvent> {
        if let (Some(c), Some(tool)) = (
            command(update),
            update["toolCallId"]
                .as_str()
                .and_then(|id| self.tools.get_mut(id)),
        ) {
            tool.command = Some(c);
        }
        let mut out: Vec<BackendEvent> = self.files(update).into_iter().collect();
        out.extend(self.tool_status(update));
        out
    }

    /// The files an edit names (CTX-2): its `locations`, and the `path` of
    /// its `diff` contents (ACP schema; ⚠️ not checked that Copilot CLI
    /// sends them).
    fn files(&self, update: &Value) -> Option<BackendEvent> {
        let tool = self.tools.get(update["toolCallId"].as_str()?)?;
        if !matches!(tool.kind.as_str(), "edit" | "delete" | "move") {
            return None;
        }
        let list = |key: &str| update[key].as_array().map_or(&[][..], Vec::as_slice);
        files_changed(
            list("locations").iter().map(|l| &l["path"]).chain(
                list("content")
                    .iter()
                    .filter(|c| c["type"] == "diff")
                    .map(|c| &c["path"]),
            ),
        )
    }

    fn tool_status(&mut self, update: &Value) -> Vec<BackendEvent> {
        let Some(id) = update["toolCallId"].as_str() else {
            return Vec::new();
        };
        let failed = match update["status"].as_str() {
            Some("completed") => false,
            Some("failed") => true,
            _ => {
                if let (Some(tool), Some(title)) =
                    (self.tools.get_mut(id), update["title"].as_str())
                {
                    title.clone_into(&mut tool.title);
                }
                return Vec::new();
            }
        };
        if self.tools.remove(id).is_none() {
            return Vec::new();
        }
        let content: Vec<Value> = update["content"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .map(|c| c["content"].clone())
            .collect();
        vec![BackendEvent::ToolDone {
            id: id.to_owned(),
            output: clip(&result_text(&Value::Array(content)), OUTPUT_MAX),
            failed,
        }]
    }
}

/// Name and detail of a tool call: Leon's verb for its kind and the agent's
/// title, or the title alone for kinds with no verb.
/// A tool call's shell command, from its `rawInput` (ACP schema; Copilot
/// CLI 1.0.114 sends it for `execute`).
pub(super) fn command(call: &Value) -> Option<String> {
    call["rawInput"]["command"]
        .as_str()
        .filter(|c| !c.trim().is_empty())
        .map(str::to_owned)
}

fn describe(tool: &OpenTool) -> (String, String, Option<String>) {
    let verb = match tool.kind.as_str() {
        "read" => "Read",
        "edit" | "delete" | "move" => "Edited",
        "search" => "Searched",
        "execute" => "Ran",
        "fetch" => "Fetched",
        _ => return (clip(&tool.title, DETAIL_MAX), String::new(), None),
    };
    // A command's title says what it is for (Copilot CLI 1.0.114: "Create
    // worktree and sync deps"); the command itself is the detail.
    match tool.command.as_deref().filter(|c| *c != tool.title) {
        Some(command) if !tool.title.is_empty() => (
            verb.to_owned(),
            clip(command, DETAIL_MAX),
            Some(clip(&tool.title, DETAIL_MAX)),
        ),
        _ => (verb.to_owned(), clip(&tool.title, DETAIL_MAX), None),
    }
}

/// Token usage from the `session/prompt` response. The field is marked
/// unstable in the spec: ⚠️ not verified that either CLI sends it.
fn usage(usage: &Value) -> Option<Usage> {
    Some(Usage {
        input_tokens: usage["inputTokens"].as_u64()?,
        output_tokens: usage["outputTokens"].as_u64()?,
        cost_usd: None,
    })
}

#[cfg(test)]
mod tests {
    use super::{OpenTool, describe};

    /// A command's title says what it is for; the row shows the command.
    #[test]
    fn a_command_keeps_its_title_as_its_purpose() {
        let tool = |title: &str, kind: &str, command: Option<&str>| OpenTool {
            title: title.into(),
            kind: kind.into(),
            command: command.map(Into::into),
        };
        let cases = [
            (
                tool("Create worktree", "execute", Some("git worktree add x")),
                ("Ran", "git worktree add x", Some("Create worktree")),
            ),
            (
                tool("cargo test", "execute", Some("cargo test")),
                ("Ran", "cargo test", None),
            ),
            (tool("src/a.rs", "read", None), ("Read", "src/a.rs", None)),
        ];
        for (tool, (verb, detail, purpose)) in cases {
            let (v, d, p) = describe(&tool);
            assert_eq!(
                (v.as_str(), d.as_str(), p.as_deref()),
                (verb, detail, purpose)
            );
        }
    }
}
