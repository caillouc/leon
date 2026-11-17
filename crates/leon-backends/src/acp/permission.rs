//! `session/request_permission`: the agent asks before a tool call; the user
//! answers with one of its options.

use leon_core::ports::BackendEvent;
use serde_json::{Value, json};

use super::turn::OpenTool;
use crate::process::clip;

/// A permission request waiting for the user's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Permission {
    /// JSON-RPC id of the agent's request.
    rpc_id: Value,
    /// `(name, optionId, kind)`, as the agent gave them.
    options: Vec<(String, String, String)>,
}

impl Permission {
    /// The question for the user and the pending request. `known` is the
    /// tool call the request refers to, when it was announced before.
    pub(super) fn ask(
        rpc_id: &Value,
        params: &Value,
        known: Option<&OpenTool>,
    ) -> (BackendEvent, Self) {
        let options: Vec<(String, String, String)> = params["options"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(|o| {
                Some((
                    o["name"].as_str()?.to_owned(),
                    o["optionId"].as_str()?.to_owned(),
                    o["kind"].as_str().unwrap_or_default().to_owned(),
                ))
            })
            .collect();
        let call = &params["toolCall"];
        let kind = call["kind"]
            .as_str()
            .or(known.map(|t| t.kind.as_str()))
            .unwrap_or_default();
        let title = match kind {
            "execute" => "Run this command?",
            "edit" | "delete" | "move" => "Apply these file changes?",
            _ => "Allow this tool call?",
        };
        let detail = call["title"]
            .as_str()
            .or(known.map(|t| t.title.as_str()))
            .unwrap_or_default();
        // The allow-once option first: the one the cards recommend.
        let mut names: Vec<String> = options.iter().map(|(n, _, _)| n.clone()).collect();
        if let Some(i) = options.iter().position(|(_, _, k)| k == "allow_once") {
            let allow = names.remove(i);
            names.insert(0, allow);
        }
        let id = rpc_id
            .as_str()
            .map_or_else(|| rpc_id.to_string(), str::to_owned);
        let question = BackendEvent::Question {
            id: format!("acp-{id}"),
            title: title.to_owned(),
            detail: clip(detail, 200),
            options: names,
        };
        let permission = Self {
            rpc_id: rpc_id.clone(),
            options,
        };
        (question, permission)
    }

    /// The response for the user's answer: the option with that name (or
    /// id); any other text rejects once.
    pub(super) fn answer(&self, answer: &str) -> Value {
        let chosen = self
            .options
            .iter()
            .find(|(name, id, _)| name == answer || id == answer)
            .or_else(|| self.options.iter().find(|(_, _, k)| k == "reject_once"));
        match chosen {
            Some((_, id, _)) => self.reply(&json!({ "outcome": "selected", "optionId": id })),
            None => self.cancel(),
        }
    }

    /// The response when the turn is cancelled (required by the spec).
    pub(super) fn cancel(&self) -> Value {
        self.reply(&json!({ "outcome": "cancelled" }))
    }

    fn reply(&self, outcome: &Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": self.rpc_id, "result": { "outcome": outcome } })
    }
}
