//! The tool calls of a turn fold into one line at the top of its answer
//! (CONV-20): the answer reads as text, its commands one click away.

use std::ops::Range;

use crate::{Entry, TimedEntry};

/// How a transcript shows one of its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// On its own line.
    Alone,
    /// The first entry of an answer with tool calls: above it, one line for
    /// all of them (the answer's entries), then this entry unless it is one.
    Turn(Range<usize>),
    /// A tool call: in its answer's line.
    Folded,
}

/// How each entry of `entries` shows (CONV-20). An answer is what follows
/// a message to the agent (the user's or another agent's) up to the end of
/// its turn; a turn its CLI starts by itself is an answer too.
pub fn fold_turns(entries: &[TimedEntry]) -> Vec<Shown> {
    let mut shown = vec![Shown::Alone; entries.len()];
    let mut ix = 0;
    while ix < entries.len() {
        if is_message(&entries[ix].entry) {
            ix += 1;
            continue;
        }
        let start = ix;
        while ix < entries.len() && !is_message(&entries[ix].entry) {
            ix += 1;
            if matches!(entries[ix - 1].entry, Entry::TurnEnd { .. }) {
                break;
            }
        }
        let mut tools = (start..ix).filter(|&i| matches!(entries[i].entry, Entry::Tool { .. }));
        if let Some(first) = tools.next() {
            shown[start] = Shown::Turn(start..ix);
            if first != start {
                shown[first] = Shown::Folded;
            }
            for i in tools {
                shown[i] = Shown::Folded;
            }
        }
    }
    shown
}

/// A message sent to the agent: what starts a turn.
fn is_message(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::User { .. } | Entry::FromAgent { .. } | Entry::ForYou { .. }
    )
}

/// What the tool calls of an answer did, for its folded line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    /// Each kind of call and how many, in the order first made:
    /// "Ran ×6 · Read ×2 · Edited".
    pub text: String,
    /// How many failed.
    pub failed: usize,
    /// One has no result yet.
    pub running: bool,
}

/// The summary of the tool calls among `entries`; other entries are
/// skipped.
pub fn run_summary(entries: &[TimedEntry]) -> RunSummary {
    let mut kinds: Vec<(&str, usize)> = Vec::new();
    let (mut failed, mut running) = (0, false);
    for e in entries {
        let Entry::Tool {
            name,
            output,
            failed: f,
            ..
        } = &e.entry
        else {
            continue;
        };
        let kind = group(name);
        match kinds.iter_mut().find(|(n, _)| *n == kind) {
            Some((_, count)) => *count += 1,
            None => kinds.push((kind, 1)),
        }
        failed += usize::from(*f);
        running |= output.is_none();
    }
    let text = kinds
        .iter()
        .map(|(name, count)| match count {
            1 => (*name).to_owned(),
            n => format!("{name} ×{n}"),
        })
        .collect::<Vec<_>>()
        .join(" · ");
    RunSummary {
        text,
        failed,
        running,
    }
}

/// What a tool call counts as in a summary: an MCP tool as its server
/// ("Slack" for Claude Code's `mcp__claude_ai_Slack__slack_read_channel`,
/// `metabase` for Codex's `metabase.search`), any other as its name.
fn group(name: &str) -> &str {
    if let Some(rest) = name.strip_prefix("mcp__")
        && let Some((server, _)) = rest.split_once("__")
    {
        // claude.ai's connectors, as Claude Code names them.
        return server.strip_prefix("claude_ai_").unwrap_or(server);
    }
    let word = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    match name.split_once('.') {
        Some((server, tool)) if word(server) && word(tool) => server,
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::{RunSummary, Shown, fold_turns, run_summary};
    use crate::{ConversationId, Entry, TimedEntry};

    fn at(entry: Entry) -> TimedEntry {
        TimedEntry { at: 0, entry }
    }

    fn tool(name: &str, output: Option<&str>, failed: bool) -> TimedEntry {
        at(Entry::Tool {
            id: String::new(),
            name: name.into(),
            detail: String::new(),
            purpose: None,
            output: output.map(Into::into),
            failed,
        })
    }

    fn ran() -> TimedEntry {
        tool("Ran", Some("ok"), false)
    }

    fn text() -> TimedEntry {
        at(Entry::Agent { text: "ok".into() })
    }

    fn you() -> TimedEntry {
        at(Entry::User { text: "go".into() })
    }

    fn end() -> TimedEntry {
        at(Entry::TurnEnd { usage: None })
    }

    #[test]
    fn the_tool_calls_of_an_answer_fold_at_its_top() {
        use Shown::{Alone, Folded, Turn};
        let asked = at(Entry::FromAgent {
            from: ConversationId(1),
            text: "check".into(),
            answer: false,
        });
        let cases: [(Vec<TimedEntry>, Vec<Shown>); 5] = [
            (vec![you(), text(), end()], vec![Alone, Alone, Alone]),
            // Wherever they were, above the answer's first words.
            (
                vec![you(), text(), ran(), text(), ran(), text(), end()],
                vec![Alone, Turn(1..7), Folded, Alone, Folded, Alone, Alone],
            ),
            // An answer starting with a call: the line takes its place.
            (
                vec![you(), ran(), text(), end()],
                vec![Alone, Turn(1..4), Alone, Alone],
            ),
            // Each answer its own line; one still going reaches the end.
            (
                vec![you(), ran(), end(), asked, ran(), ran()],
                vec![Alone, Turn(1..3), Alone, Alone, Turn(4..6), Folded],
            ),
            // A turn the CLI started by itself, after the previous one.
            (
                vec![you(), text(), end(), ran(), text(), end()],
                vec![Alone, Alone, Alone, Turn(3..6), Alone, Alone],
            ),
        ];
        for (entries, expected) in cases {
            assert_eq!(fold_turns(&entries), expected, "{entries:?}");
        }
    }

    /// MCP tools count as their server: one entry, not one per tool.
    #[test]
    fn mcp_tools_count_as_their_server() {
        let run = [
            tool("Skill", Some(""), false),
            tool(
                "mcp__claude_ai_Slack__slack_search_channels",
                Some(""),
                false,
            ),
            tool("mcp__claude_ai_Slack__slack_read_channel", Some(""), false),
            tool("mcp__leon__rename_flow", Some(""), false),
            tool("metabase.search", Some(""), false),
            tool("metabase.query", Some(""), false),
            ran(),
            tool("Viewing src/a.rs", Some(""), false),
        ];
        assert_eq!(
            run_summary(&run).text,
            "Skill · Slack ×2 · leon · metabase ×2 · Ran · Viewing src/a.rs"
        );
    }

    #[test]
    fn a_run_says_what_it_did() {
        let run = [
            ran(),
            text(),
            tool("Read", Some("x"), false),
            tool("Ran", Some("error"), true),
            tool("Edited", Some("ok"), false),
            ran(),
        ];
        assert_eq!(
            run_summary(&run),
            RunSummary {
                text: "Ran ×3 · Read · Edited".into(),
                failed: 1,
                running: false,
            }
        );
        let going = [ran(), tool("Ran", None, false)];
        assert_eq!(
            run_summary(&going),
            RunSummary {
                text: "Ran ×2".into(),
                failed: 0,
                running: true,
            }
        );
    }
}
