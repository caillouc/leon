//! `session/update` notifications of a turn turned into Leon events.

use std::collections::HashMap;

use leon_core::ports::BackendEvent;
use leon_domain::Usage;
use serde_json::Value;

use crate::process::clip;
use crate::summary::{OUTPUT_MAX, result_text};

const DETAIL_MAX: usize = 200;

/// A tool call that has not finished.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct OpenTool {
    pub title: String,
    pub kind: String,
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
        };
        let (name, detail) = describe(&tool);
        out.push(BackendEvent::Tool {
            id: id.to_owned(),
            name,
            detail,
        });
        self.tools.insert(id.to_owned(), tool);
        out.extend(self.tool_update(update));
        out
    }

    fn tool_update(&mut self, update: &Value) -> Vec<BackendEvent> {
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
fn describe(tool: &OpenTool) -> (String, String) {
    let verb = match tool.kind.as_str() {
        "read" => "Read",
        "edit" | "delete" | "move" => "Edited",
        "search" => "Searched",
        "execute" => "Ran",
        "fetch" => "Fetched",
        _ => return (clip(&tool.title, DETAIL_MAX), String::new()),
    };
    (verb.to_owned(), clip(&tool.title, DETAIL_MAX))
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
