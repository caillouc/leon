use serde::{Deserialize, Serialize};

use crate::AgentSessionId;

/// One line of an agent session's transcript. Persisted (P5): new variants
/// only, never change the meaning of an existing one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// A message the user sent.
    User {
        /// Message text.
        text: String,
    },
    /// A message another agent session sent to this one: a request, a
    /// follow-up, or the answer to a request (FLOW-1, FLOW-2).
    FromAgent {
        /// The session that sent it.
        from: AgentSessionId,
        /// Message text.
        text: String,
        /// `true`: the answer to a request this session sent; `false`: a
        /// request, whose answer goes back to `from`.
        answer: bool,
    },
    /// A complete message from the agent.
    Agent {
        /// Markdown text.
        text: String,
    },
    /// A tool the agent called (collapsed in the UI).
    Tool {
        /// CLI's id for the call, to attach its result.
        id: String,
        /// What it did: "Ran", "Edited", a tool name…
        name: String,
        /// Short detail: a command, a path.
        detail: String,
        /// Output, once known.
        output: Option<String>,
        /// The tool reported a failure.
        failed: bool,
    },
    /// A note from Leon: "Stopped by you", a setting change.
    Notice {
        /// Note text.
        text: String,
    },
    /// The CLI reported an error or stopped.
    Error {
        /// What went wrong.
        text: String,
    },
    /// The end of a turn, with its usage when the CLI reports it.
    TurnEnd {
        /// Token and cost figures.
        usage: Option<Usage>,
    },
}

/// Usage of one turn, as the CLI reports it (USE-1).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Input tokens, cache included.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
    /// Cost estimate in USD when the CLI gives one.
    pub cost_usd: Option<f64>,
}

impl Usage {
    /// The usage of every turn of a transcript, added up (USE-1). The cost
    /// is known only when every turn reported one.
    pub fn total(entries: &[TimedEntry]) -> Self {
        entries
            .iter()
            .filter_map(|e| match &e.entry {
                Entry::TurnEnd { usage } => *usage,
                _ => None,
            })
            .fold(
                Self {
                    cost_usd: Some(0.),
                    ..Self::default()
                },
                |a, b| Self {
                    input_tokens: a.input_tokens + b.input_tokens,
                    output_tokens: a.output_tokens + b.output_tokens,
                    cost_usd: a.cost_usd.zip(b.cost_usd).map(|(x, y)| x + y),
                },
            )
    }
}

/// A transcript entry with its time (Unix milliseconds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedEntry {
    /// When it happened.
    pub at: i64,
    /// What happened.
    pub entry: Entry,
}

#[cfg(test)]
mod tests {
    use super::{Entry, TimedEntry, Usage};

    fn end(input: u64, cost: Option<f64>) -> TimedEntry {
        TimedEntry {
            at: 0,
            entry: Entry::TurnEnd {
                usage: Some(Usage {
                    input_tokens: input,
                    output_tokens: 1,
                    cost_usd: cost,
                }),
            },
        }
    }

    #[test]
    fn usage_adds_up_turns() {
        let text = TimedEntry {
            at: 0,
            entry: Entry::Agent { text: "hi".into() },
        };
        let total = Usage::total(&[end(10, Some(0.5)), text, end(5, Some(0.25))]);
        assert_eq!(
            (total.input_tokens, total.output_tokens, total.cost_usd),
            (15, 2, Some(0.75))
        );
        assert_eq!(
            Usage::total(&[end(10, Some(0.5)), end(5, None)]).cost_usd,
            None
        );
        assert_eq!(Usage::total(&[]).input_tokens, 0);
    }

    #[test]
    fn entries_have_a_stable_json_form() {
        let entry = Entry::Tool {
            id: "t1".into(),
            name: "Ran".into(),
            detail: "ls".into(),
            output: None,
            failed: false,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"tool","id":"t1","name":"Ran","detail":"ls","output":null,"failed":false}"#
        );
        let back: Entry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn turn_end_round_trips() {
        let entry = Entry::TurnEnd {
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: 2,
                cost_usd: None,
            }),
        };
        let back: Entry = serde_json::from_str(&serde_json::to_string(&entry).unwrap()).unwrap();
        assert_eq!(back, entry);
    }
}
