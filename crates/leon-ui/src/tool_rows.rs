//! The tool calls of a transcript: a row each, unfolded to its whole
//! command and its output; those of an answer gather at its top, two or
//! more folded into one line saying what they did (CONV-20).

use std::collections::HashSet;
use std::ops::Range;

use gpui::{AnyElement, App, ElementId, WeakEntity, div, prelude::*};
use leon_domain::{ConversationId, Entry, TimedEntry, run_summary};

use crate::icons::chevron;
use crate::theme;
use crate::workspace::Workspace;

/// An entry of a conversation: its id and its index in the transcript.
type At = (ConversationId, usize);

/// One of the workspace's sets of things the user unfolded.
type Unfolded = fn(&mut Workspace) -> &mut HashSet<At>;

/// Folds or unfolds `at` in the set `unfolded` picks.
pub(crate) fn toggle(view: &WeakEntity<Workspace>, unfolded: Unfolded, at: At, cx: &mut App) {
    let _ = view.update(cx, |this, cx| {
        let set = unfolded(this);
        if !set.remove(&at) {
            set.insert(at);
        }
        cx.notify();
    });
}

/// A tool's detail on one line: its first, "…" when more follow.
pub(crate) fn one_line(detail: &str) -> String {
    let mut lines = detail.lines();
    let first = lines.next().unwrap_or_default();
    if lines.next().is_some() {
        format!("{first} …")
    } else {
        first.to_owned()
    }
}

/// A tool call: what it is for (else its name) and the start of its
/// detail on one line; unfolded, its whole detail (the command, the input)
/// and its output. Nothing for another entry.
pub(crate) fn tool_row(
    (conversation, ix): At,
    call: &Entry,
    expanded: bool,
    view: &WeakEntity<Workspace>,
) -> AnyElement {
    let Entry::Tool {
        name,
        detail,
        purpose,
        output,
        failed,
        ..
    } = call
    else {
        return div().into_any_element();
    };
    let (detail, output, failed) = (detail.as_str(), output.as_deref(), *failed);
    let view = view.clone();
    let mut tool = div()
        .id(ElementId::Name(
            format!("tool-{}-{ix}", conversation.0).into(),
        ))
        .text_size(crate::theme::rem(12.))
        .text_color(if failed {
            theme::err()
        } else {
            theme::fg_muted()
        })
        .cursor_pointer()
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(chevron(expanded, theme::fg_muted(), 12.))
                // "Check the Redis load" says more than "Ran" (CONV-20).
                .child(
                    div()
                        .flex_none()
                        .max_w(gpui::relative(0.6))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(theme::fg())
                        .child(purpose.clone().unwrap_or_else(|| name.clone())),
                )
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(one_line(detail)),
                )
                .when(output.is_none(), |d| {
                    d.child(div().text_color(theme::accent()).child("running"))
                }),
        )
        .on_click(move |_, _, cx| {
            toggle(&view, |w| &mut w.expanded, (conversation, ix), cx);
        });
    if expanded {
        let output = output.filter(|o| !o.is_empty()).unwrap_or("No output");
        tool = tool.child(
            div()
                .mt_1()
                .ml_4()
                .rounded_md()
                .bg(theme::surface())
                .border_1()
                .border_color(theme::border())
                .font_family(theme::MONO)
                .text_size(crate::theme::rem(11.5))
                // Whole, where the row shows its start.
                .when(!detail.is_empty(), |d| {
                    d.child(
                        div()
                            .p_2()
                            .border_b_1()
                            .border_color(theme::border())
                            .text_color(theme::fg())
                            .child(detail.to_owned()),
                    )
                })
                .child(div().p_2().child(output.to_owned())),
        );
    }
    tool.into_any_element()
}

/// The tool calls of an answer, `turn` in `entries`, at its top: one alone
/// keeps its row; more fold into one line saying what they did, unfolded
/// to their rows, each unfolding on its own.
pub(crate) fn turn_tools(
    conversation: ConversationId,
    turn: Range<usize>,
    entries: &[TimedEntry],
    open_runs: &HashSet<At>,
    expanded: &HashSet<At>,
    view: &WeakEntity<Workspace>,
) -> AnyElement {
    let start = turn.start;
    let answer = entries.get(turn.clone()).unwrap_or_default();
    let rows = || {
        turn.clone()
            .zip(answer)
            .filter(|(_, e)| matches!(e.entry, Entry::Tool { .. }))
            .map(|(ix, e)| {
                let open = expanded.contains(&(conversation, ix));
                tool_row((conversation, ix), &e.entry, open, view)
            })
    };
    let mut one = rows();
    if let (Some(only), None) = (one.next(), one.next()) {
        return only;
    }
    let open = open_runs.contains(&(conversation, start));
    let summary = run_summary(answer);
    let toggle_view = view.clone();
    let line = div()
        .id(ElementId::Name(
            format!("run-{}-{start}", conversation.0).into(),
        ))
        .flex()
        .gap_2()
        .items_center()
        .cursor_pointer()
        .child(chevron(open, theme::fg_muted(), 12.))
        // Muted: Leon's note, not the agent's words.
        .child(summary.text)
        .when(summary.failed > 0, |d| {
            d.child(
                div()
                    .text_color(theme::err())
                    .child(format!("{} failed", summary.failed)),
            )
        })
        .when(summary.running, |d| {
            d.child(div().text_color(theme::accent()).child("running"))
        })
        .on_click(move |_, _, cx| {
            toggle(
                &toggle_view,
                |w| &mut w.open_runs,
                (conversation, start),
                cx,
            );
        });
    div()
        .text_size(crate::theme::rem(12.))
        .text_color(theme::fg_muted())
        .child(line)
        // Under the line's text, past its chevron and gap.
        .when(open, |d| {
            d.child(
                div()
                    .pl(crate::theme::rem(20.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(rows()),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::one_line;

    #[test]
    fn a_detail_shows_its_first_line() {
        let cases = [
            ("", ""),
            ("cargo test", "cargo test"),
            ("cd /r && \\\n  cargo test", "cd /r && \\ …"),
        ];
        for (detail, expected) in cases {
            assert_eq!(one_line(detail), expected, "{detail}");
        }
    }
}
