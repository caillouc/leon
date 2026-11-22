//! Claude Code's questions and approval requests, as cards to answer
//! (CONV-9). Started with `--permission-prompt-tool stdio`, Claude Code asks
//! on stdout with a `control_request` of subtype `can_use_tool` and waits for
//! a `control_response` on stdin; without that flag it removes its question
//! tool, `AskUserQuestion` (both seen on real runs of 2.1.288, 2026-11-20).

use leon_core::ports::BackendEvent;
use serde_json::{Map, Value, json};

use crate::summary::claude_tool;

/// The answers to an approval request.
const ALLOW: &str = "Allow once";
const DENY: &str = "Deny";

/// What Claude Code waits for an answer to: at most one request at a time.
#[derive(Debug, Default)]
pub(crate) struct Asks {
    pending: Option<Pending>,
}

#[derive(Debug)]
struct Pending {
    request_id: String,
    /// The tool's input, sent back with the answers.
    input: Value,
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    /// `AskUserQuestion`: its questions, asked one after another, and the
    /// answers so far, by question.
    Questions {
        questions: Vec<Asked>,
        answers: Map<String, Value>,
    },
    /// Another tool, to allow or deny: its title and what it would do.
    Approval { title: String, detail: String },
}

#[derive(Debug)]
struct Asked {
    text: String,
    /// Each option's label and description.
    options: Vec<(String, String)>,
}

impl Asks {
    /// A `control_request` from Claude Code: the card to show, if it asks
    /// for one.
    pub(crate) fn request(&mut self, v: &Value) -> Vec<BackendEvent> {
        let request = &v["request"];
        let (Some("can_use_tool"), Some(request_id)) =
            (request["subtype"].as_str(), v["request_id"].as_str())
        else {
            return Vec::new();
        };
        let input = request["input"].clone();
        let tool = request["tool_name"].as_str().unwrap_or_default();
        let kind = if tool == "AskUserQuestion" {
            Kind::Questions {
                questions: questions(&input),
                answers: Map::new(),
            }
        } else {
            let (_, detail) = claude_tool(tool, &input);
            Kind::Approval {
                title: approval_title(tool),
                detail,
            }
        };
        self.pending = Some(Pending {
            request_id: request_id.to_owned(),
            input,
            kind,
        });
        self.shown().into_iter().collect()
    }

    /// Claude Code waits for an answer.
    pub(crate) fn waiting(&self) -> bool {
        self.pending.is_some()
    }

    /// The user's answer — a click on the card `id`, or a message typed
    /// while a card shows (`None`): the next card, or the line that answers
    /// Claude Code. A click on a card no longer shown does nothing.
    pub(crate) fn answer(
        &mut self,
        id: Option<&str>,
        answer: &str,
    ) -> (Vec<BackendEvent>, Option<String>) {
        let shown = self.shown_id();
        let Some(pending) = self.pending.as_mut() else {
            return (Vec::new(), None);
        };
        if id.is_some_and(|id| Some(id) != shown.as_deref()) {
            return (Vec::new(), None);
        }
        let decision = match &mut pending.kind {
            Kind::Questions { questions, answers } => {
                if let Some(asked) = questions.get(answers.len()) {
                    answers.insert(asked.text.clone(), Value::String(answer.to_owned()));
                }
                if answers.len() < questions.len() {
                    return (self.shown().into_iter().collect(), None);
                }
                let mut input = pending.input.clone();
                input["answers"] = Value::Object(std::mem::take(answers));
                json!({ "behavior": "allow", "updatedInput": input })
            }
            Kind::Approval { .. } => match answer {
                ALLOW => json!({ "behavior": "allow", "updatedInput": pending.input }),
                DENY => json!({ "behavior": "deny", "message": "The user denied this." }),
                said => json!({
                    "behavior": "deny",
                    "message": format!("The user denied this and said: {said}"),
                }),
            },
        };
        let line = response(&pending.request_id, &decision);
        self.pending = None;
        (Vec::new(), Some(line))
    }

    /// The card to show now.
    fn shown(&self) -> Option<BackendEvent> {
        let pending = self.pending.as_ref()?;
        let id = self.shown_id()?;
        Some(match &pending.kind {
            Kind::Questions { questions, answers } => {
                let asked = questions.get(answers.len())?;
                BackendEvent::Question {
                    id,
                    title: asked.text.clone(),
                    detail: asked
                        .options
                        .iter()
                        .filter(|(_, about)| !about.is_empty())
                        .map(|(label, about)| format!("{label}: {about}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    options: asked.options.iter().map(|(l, _)| l.clone()).collect(),
                }
            }
            Kind::Approval { title, detail } => BackendEvent::Question {
                id,
                title: title.clone(),
                detail: detail.clone(),
                options: vec![ALLOW.to_owned(), DENY.to_owned()],
            },
        })
    }

    /// The shown card's id: the request's, numbered for each question.
    fn shown_id(&self) -> Option<String> {
        let pending = self.pending.as_ref()?;
        Some(match &pending.kind {
            Kind::Questions { answers, .. } => format!("{}#{}", pending.request_id, answers.len()),
            Kind::Approval { .. } => pending.request_id.clone(),
        })
    }
}

/// `AskUserQuestion`'s questions with their options.
fn questions(input: &Value) -> Vec<Asked> {
    input["questions"].as_array().map_or_default(|list| {
        list.iter()
            .filter_map(|q| {
                let options = q["options"].as_array().map_or_default(|o| {
                    o.iter()
                        .filter_map(|o| {
                            let label = o["label"].as_str()?.to_owned();
                            let about = o["description"].as_str().unwrap_or_default();
                            Some((label, about.to_owned()))
                        })
                        .collect()
                });
                Some(Asked {
                    text: q["question"].as_str()?.to_owned(),
                    options,
                })
            })
            .collect()
    })
}

/// What an approval card asks, by tool, as for Codex's.
fn approval_title(tool: &str) -> String {
    match tool {
        "Bash" => "Run this command?".to_owned(),
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => "Change this file?".to_owned(),
        other => format!("Allow {other}?"),
    }
}

/// The `control_response` line answering request `request_id`.
fn response(request_id: &str, decision: &Value) -> String {
    let message = json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": decision },
    });
    format!("{message}\n")
}

#[cfg(test)]
mod tests;
