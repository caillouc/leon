use serde::{Deserialize, Serialize};

use crate::ConversationId;

/// One line of an conversation's transcript. Persisted (P5): new variants
/// only, never change the meaning of an existing one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// A message the user sent.
    User {
        /// Message text.
        text: String,
    },
    /// A message another conversation sent to this one: a request, a
    /// follow-up, or the answer to a request (FLOW-1, FLOW-2).
    FromAgent {
        /// The conversation that sent it.
        from: ConversationId,
        /// Message text.
        text: String,
        /// `true`: the answer to a request this conversation sent; `false`: a
        /// request, whose answer goes back to `from`.
        answer: bool,
    },
    /// The first message of a conversation another one started for the
    /// user: its answers go to the user, who reviews and continues it, not
    /// back to `from` (FLOW-1).
    ForYou {
        /// The conversation that started it.
        from: ConversationId,
        /// Message text.
        text: String,
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
            .fold(Self::nothing(), Self::plus)
    }

    /// No usage yet, at no cost: what adding starts from.
    pub fn nothing() -> Self {
        Self {
            cost_usd: Some(0.),
            ..Self::default()
        }
    }

    /// Two usages added up; the cost is known only when both are.
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cost_usd: self.cost_usd.zip(other.cost_usd).map(|(x, y)| x + y),
        }
    }
}

/// Who a conversation's answers go to after `entry`, `before` the one
/// they went to until then (FLOW-1): an agent's request sends them to that
/// agent, until the user writes to the conversation or it is handed to the
/// user (`None`, the user); an answer it receives from an agent of its own,
/// or anything else, changes nothing. Leon routes answers and draws the
/// flow's arrows by this one rule.
pub fn answers_to(before: Option<ConversationId>, entry: &Entry) -> Option<ConversationId> {
    match entry {
        Entry::FromAgent {
            from,
            answer: false,
            ..
        } => Some(*from),
        Entry::User { .. } | Entry::ForYou { .. } => None,
        _ => before,
    }
}

/// Whether the user talks with a conversation (FLOW-3): they wrote to it,
/// or an agent started it for them.
pub fn talks_with_user(entries: &[TimedEntry]) -> bool {
    entries
        .iter()
        .any(|e| matches!(e.entry, Entry::User { .. } | Entry::ForYou { .. }))
}

/// What a transcript holds, as the flow diagram counts it (FLOW-4): its
/// messages (the user's, the agent's, other agents') and its tool calls.
pub fn activity(entries: &[TimedEntry]) -> (usize, usize) {
    entries
        .iter()
        .fold((0, 0), |(messages, tools), e| match e.entry {
            Entry::Tool { .. } => (messages, tools + 1),
            Entry::User { .. }
            | Entry::Agent { .. }
            | Entry::FromAgent { .. }
            | Entry::ForYou { .. } => (messages + 1, tools),
            Entry::Notice { .. } | Entry::Error { .. } | Entry::TurnEnd { .. } => (messages, tools),
        })
}

/// A transcript entry with its time (Unix milliseconds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedEntry {
    /// When it happened.
    pub at: i64,
    /// What happened.
    pub entry: Entry,
}

/// A tool call still running: what, on what, and since when (Unix ms).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningTool {
    /// What it does: "Ran", "Edited", a tool name.
    pub name: String,
    /// A command, a path.
    pub detail: String,
    /// When it started.
    pub since: i64,
}

/// What a tool call says when its turn ended before its result: the user
/// stopped the turn, or the CLI never gave one.
pub const NO_RESULT: &str = "No result: its turn ended first";

impl Entry {
    /// A tool call still without a result gets [`NO_RESULT`]: its turn is
    /// over, so it no longer runs.
    pub fn end_tool(&mut self) {
        if let Self::Tool {
            output: output @ None,
            ..
        } = self
        {
            *output = Some(NO_RESULT.to_owned());
        }
    }
}

/// Ends the tool calls left without a result before the transcript's last
/// turn end, as saved before a turn's end ended them.
pub fn end_unfinished(entries: &mut [TimedEntry]) {
    let turn_end = |e: &TimedEntry| matches!(e.entry, Entry::TurnEnd { .. });
    let Some(last) = entries.iter().rposition(turn_end) else {
        return;
    };
    for e in &mut entries[..last] {
        e.entry.end_tool();
    }
}

/// The tool calls of a transcript that have no result yet, oldest first.
pub fn running_tools(entries: &[TimedEntry]) -> Vec<RunningTool> {
    entries
        .iter()
        .filter_map(|e| match &e.entry {
            Entry::Tool {
                name,
                detail,
                output: None,
                ..
            } => Some(RunningTool {
                name: name.clone(),
                detail: detail.clone(),
                since: e.at,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    /// Tools without a result before the last turn end are ended; one in
    /// the turn still going on, or with a result, is left as it is.
    #[test]
    fn tools_left_without_a_result_end_with_their_turn() {
        use super::{Entry, NO_RESULT, TimedEntry, end_unfinished};
        let tool = |output: Option<&str>| Entry::Tool {
            id: String::new(),
            name: "Read".into(),
            detail: String::new(),
            output: output.map(str::to_owned),
            failed: false,
        };
        let end = || Entry::TurnEnd { usage: None };
        let cases = [
            (
                vec![tool(None), end(), tool(None)],
                vec![tool(Some(NO_RESULT)), end(), tool(None)],
            ),
            (vec![tool(Some("ok")), end()], vec![tool(Some("ok")), end()]),
            (vec![tool(None)], vec![tool(None)]),
        ];
        for (before, expected) in cases {
            let mut entries: Vec<TimedEntry> = before
                .into_iter()
                .map(|entry| TimedEntry { at: 0, entry })
                .collect();
            end_unfinished(&mut entries);
            let after: Vec<Entry> = entries.into_iter().map(|e| e.entry).collect();
            assert_eq!(after, expected);
        }
    }

    #[test]
    fn answers_go_to_the_asker_until_the_user_writes() {
        use super::{Entry, answers_to};
        use crate::ConversationId;
        let (planner, tester) = (ConversationId(1), ConversationId(3));
        let request = |from| Entry::FromAgent {
            from,
            text: String::new(),
            answer: false,
        };
        let answer = |from| Entry::FromAgent {
            from,
            text: String::new(),
            answer: true,
        };
        let user = Entry::User {
            text: String::new(),
        };
        let for_you = Entry::ForYou {
            from: planner,
            text: String::new(),
        };
        let cases = [
            (None, request(planner), Some(planner)),
            (Some(planner), answer(tester), Some(planner)),
            (Some(planner), Entry::TurnEnd { usage: None }, Some(planner)),
            (Some(planner), user, None),
            (Some(planner), for_you, None),
            (None, answer(tester), None),
        ];
        for (before, entry, expected) in cases {
            assert_eq!(answers_to(before, &entry), expected, "{entry:?}");
        }
    }

    #[test]
    fn the_user_talks_with_a_conversation_they_wrote_to_or_got_from_an_agent() {
        use super::talks_with_user;
        use crate::ConversationId;
        let at = |entry| TimedEntry { at: 0, entry };
        let text = String::new;
        let cases = [
            (vec![], false),
            (
                vec![Entry::FromAgent {
                    from: ConversationId(1),
                    text: text(),
                    answer: false,
                }],
                false,
            ),
            (
                vec![Entry::Agent { text: text() }, Entry::User { text: text() }],
                true,
            ),
            (
                vec![Entry::ForYou {
                    from: ConversationId(1),
                    text: text(),
                }],
                true,
            ),
        ];
        for (entries, expected) in cases {
            let entries: Vec<TimedEntry> = entries.into_iter().map(at).collect();
            assert_eq!(talks_with_user(&entries), expected, "{entries:?}");
        }
    }

    use super::{Entry, RunningTool, TimedEntry, Usage, running_tools};

    #[test]
    fn running_tools_are_the_calls_without_a_result() {
        let tool = |at, name: &str, output: Option<&str>| TimedEntry {
            at,
            entry: Entry::Tool {
                id: format!("t{at}"),
                name: name.into(),
                detail: "cargo test".into(),
                output: output.map(Into::into),
                failed: false,
            },
        };
        let entries = [
            tool(1, "Ran", Some("ok")),
            tool(2, "Ran", None),
            tool(3, "Edited", None),
        ];
        let names: Vec<(String, i64)> = running_tools(&entries)
            .into_iter()
            .map(|RunningTool { name, since, .. }| (name, since))
            .collect();
        assert_eq!(names, [("Ran".to_owned(), 2), ("Edited".to_owned(), 3)]);
        assert_eq!(running_tools(&entries[..1]), Vec::new());
    }

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
    fn a_message_for_you_round_trips() {
        let entry = Entry::ForYou {
            from: crate::ConversationId(1),
            text: "Plan the retry fix".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(
            json,
            r#"{"kind":"for_you","from":1,"text":"Plan the retry fix"}"#
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
