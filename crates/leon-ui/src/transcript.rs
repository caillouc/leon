//! An conversation's transcript, virtualised (only visible entries are
//! laid out), following the end while the agent writes.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    AnyElement, Context, ElementId, IntoElement, ListOffset, SharedString, div, list, prelude::*,
};
use leon_core::TranscriptView;
use leon_domain::{ConversationId, Entry, TimedEntry};

use crate::icons::chevron;
use crate::markdown::markdown;
use crate::theme;
use crate::widgets::empty;
use crate::workspace::Workspace;

impl Workspace {
    /// The transcript, and the button back to the user's last message, for
    /// the line above the message box.
    pub(crate) fn render_transcript(
        &mut self,
        conversation: ConversationId,
        cx: &mut Context<Self>,
    ) -> (AnyElement, Option<AnyElement>) {
        let Some(transcript) = self.snapshot.transcripts.get(&conversation).cloned() else {
            return (
                div().flex_1().child(empty("Loading…")).into_any_element(),
                None,
            );
        };
        let streaming = transcript.partial.is_some();
        let count = transcript.entries.len() + usize::from(streaming);
        if count == 0 {
            return (
                div()
                    .flex_1()
                    .child(empty("No messages yet."))
                    .into_any_element(),
                None,
            );
        }
        let state = self.list_for(conversation, count, streaming);
        let view = cx.entity().downgrade();
        let names: Arc<HashMap<ConversationId, String>> = Arc::new(
            self.snapshot
                .conversations
                .values()
                .map(|s| (s.conversation.id, s.name.clone()))
                .collect(),
        );
        let expanded = self.expanded.clone();
        // Back to the user's last message once an answer follows it, to read
        // the answer from its start: a button, and where an unread
        // conversation opens.
        let mine = last_mine(&transcript.entries).filter(|&ix| ix + 1 < count);
        if self.open_at_mine == Some(conversation) {
            self.open_at_mine = None;
            if let Some(item_ix) = mine {
                state.scroll_to(ListOffset {
                    item_ix,
                    offset_in_item: gpui::px(0.),
                });
            }
        }
        let jump = mine.map(|item_ix| {
            let state = state.clone();
            div()
                .id("jump-to-mine")
                // Shortened before the message box's settings are.
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .px_2()
                .py(crate::theme::rem(3.))
                .rounded_md()
                .text_size(crate::theme::rem(12.))
                .text_color(theme::fg_muted())
                .cursor_pointer()
                .hover(|s| s.text_color(theme::fg()).bg(theme::surface_2()))
                .child("↑ Your last message")
                .on_click(move |_, _, _| {
                    state.scroll_to(ListOffset {
                        item_ix,
                        offset_in_item: gpui::px(0.),
                    });
                })
                .into_any_element()
        });
        let list = list(state, move |ix, _window, _cx| {
            let item = match transcript.entries.get(ix) {
                Some(entry) => entry_view(
                    conversation,
                    ix,
                    entry,
                    expanded.contains(&(conversation, ix)),
                    &view,
                    &names,
                ),
                None => partial_view(conversation, &transcript),
            };
            crate::widgets::centered(theme::COLUMN, div().px_6().py_1().child(item))
                .into_any_element()
        })
        .flex_1()
        .min_h_0()
        .py_3();
        (
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(list)
                .into_any_element(),
            jump,
        )
    }
}

/// Where the user's last message is in a transcript.
fn last_mine(entries: &[TimedEntry]) -> Option<usize> {
    entries
        .iter()
        .rposition(|e| matches!(e.entry, Entry::User { .. }))
}

fn partial_view(conversation: ConversationId, t: &Arc<TranscriptView>) -> AnyElement {
    let text = t.partial.clone().unwrap_or_default();
    div()
        .child(markdown(
            ElementId::Name(format!("partial-{}", conversation.0).into()),
            &text,
        ))
        .into_any_element()
}

fn entry_view(
    conversation: ConversationId,
    ix: usize,
    timed: &TimedEntry,
    expanded: bool,
    view: &gpui::WeakEntity<Workspace>,
    names: &HashMap<ConversationId, String>,
) -> AnyElement {
    let id = |kind: &str| ElementId::Name(format!("{kind}-{}-{ix}", conversation.0).into());
    // A message from another conversation, under who sent it.
    let from_agent = |what: &str, from: &ConversationId, text: &str| {
        let name = names
            .get(from)
            .cloned()
            .unwrap_or_else(|| format!("conversation {}", from.0));
        div()
            .my_1()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_l_2()
            .border_color(theme::border())
            .bg(theme::surface())
            .child(
                div()
                    .text_size(crate::theme::rem(11.5))
                    .text_color(theme::accent())
                    .child(format!("{what} {name}")),
            )
            .child(long_text(
                conversation,
                ix,
                id("from-agent"),
                text,
                expanded,
                view,
            ))
            .into_any_element()
    };
    match &timed.entry {
        Entry::User { text } => {
            let (view_edit, text_edit) = (view.clone(), text.clone());
            let group: SharedString = format!("user-{ix}").into();
            // "Edit from here" floats in the top-right corner on hover, so a
            // one-line message keeps a one-line box.
            // The user's messages stand out in a long transcript: tinted,
            // in the accent colour, under "You".
            div()
                .relative()
                .my_2()
                .px_3()
                .py(crate::theme::rem(9.))
                .rounded_lg()
                .border_1()
                .border_color(gpui::Rgba {
                    a: 0.5,
                    ..theme::accent()
                })
                .bg(theme::accent_soft())
                .group(group.clone())
                .child(
                    div()
                        .text_size(crate::theme::rem(11.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme::accent())
                        .child("You"),
                )
                // As typed: its line breaks kept.
                .child(long_text(
                    conversation,
                    ix,
                    id("user"),
                    &crate::markdown::hard_breaks(text),
                    expanded,
                    view,
                ))
                .child(
                    div()
                        .id(id("edit"))
                        .absolute()
                        .top(crate::theme::rem(6.))
                        .right(crate::theme::rem(8.))
                        .px_2()
                        .py(crate::theme::rem(2.))
                        .rounded_md()
                        .bg(theme::surface_2())
                        .text_size(crate::theme::rem(11.5))
                        .text_color(theme::fg_muted())
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(1.))
                        .cursor_pointer()
                        .child("Edit from here")
                        .on_click(move |_, window, cx| {
                            let text = text_edit.clone();
                            let _ = view_edit.update(cx, |this, cx| {
                                this.start_edit(conversation, ix, &text, window, cx);
                            });
                        }),
                )
                .into_any_element()
        }
        Entry::FromAgent { from, text, answer } => from_agent(
            if *answer {
                "Answer from"
            } else {
                "Message from"
            },
            from,
            text,
        ),
        // Started for the user: its answers go to them (FLOW-1).
        Entry::ForYou { from, text } => from_agent("For you, from", from, text),
        Entry::Agent { text } => div()
            .my_1()
            .child(markdown(id("agent"), text))
            .into_any_element(),
        Entry::Tool {
            name,
            detail,
            output,
            failed,
            ..
        } => {
            let view = view.clone();
            let mut tool = div()
                .id(id("tool"))
                .text_size(crate::theme::rem(12.))
                .text_color(if *failed {
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
                        .child(div().text_color(theme::fg()).child(name.clone()))
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(detail.clone()),
                        )
                        .when(output.is_none(), |d| {
                            d.child(div().text_color(theme::accent()).child("running"))
                        }),
                )
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        if !this.expanded.remove(&(conversation, ix)) {
                            this.expanded.insert((conversation, ix));
                        }
                        cx.notify();
                    });
                });
            if expanded {
                let text = output
                    .clone()
                    .filter(|o| !o.is_empty())
                    .unwrap_or_else(|| "No output".to_owned());
                tool = tool.child(
                    div()
                        .mt_1()
                        .ml_4()
                        .p_2()
                        .rounded_md()
                        .bg(theme::surface())
                        .border_1()
                        .border_color(theme::border())
                        .font_family(theme::MONO)
                        .text_size(crate::theme::rem(11.5))
                        .child(text),
                );
            }
            tool.into_any_element()
        }
        Entry::Notice { text } => div()
            .my_1()
            .flex()
            .justify_center()
            .text_size(crate::theme::rem(12.))
            .text_color(theme::fg_faint())
            .child(text.clone())
            .into_any_element(),
        Entry::Error { text } => div()
            .my_1()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(theme::err())
            .text_size(crate::theme::rem(12.5))
            .child(text.clone())
            .into_any_element(),
        Entry::TurnEnd { usage } => {
            let text = usage.map_or_default(usage_text);
            div()
                .h(crate::theme::rem(14.))
                .flex()
                .text_size(crate::theme::rem(10.5))
                .text_color(theme::fg_faint())
                .child(text)
                .into_any_element()
        }
    }
}

/// A message longer than [`FOLD_LINES`] lines or [`FOLD_CHARS`] characters
/// shows its start and a toggle (CONV-12).
const FOLD_LINES: usize = 12;
const FOLD_CHARS: usize = 900;
/// How much of a folded message shows: lines, characters.
const START: (usize, usize) = (6, 400);

fn long_text(
    conversation: ConversationId,
    ix: usize,
    id: ElementId,
    text: &str,
    expanded: bool,
    view: &gpui::WeakEntity<Workspace>,
) -> AnyElement {
    let lines = text.lines().count();
    if lines <= FOLD_LINES && text.chars().count() <= FOLD_CHARS {
        return markdown(id, text);
    }
    let shown = if expanded {
        text.to_owned()
    } else {
        crate::markdown::first_lines(text, START.0, START.1)
    };
    let view = view.clone();
    div()
        .child(markdown(id, &shown))
        .child(
            div()
                .id(("fold", ix))
                .mt_1()
                .text_size(crate::theme::rem(12.))
                .text_color(theme::accent())
                .cursor_pointer()
                .child(if expanded {
                    "Show less".to_owned()
                } else {
                    format!("Show all · {lines} lines")
                })
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        if !this.expanded.remove(&(conversation, ix)) {
                            this.expanded.insert((conversation, ix));
                        }
                        cx.notify();
                    });
                }),
        )
        .into_any_element()
}

/// "41.2k in · 1.8k out · $0.42" (USE-1).
pub(crate) fn usage_text(u: leon_domain::Usage) -> String {
    let cost = u.cost_usd.map_or_default(|c| format!(" · ${c:.2}"));
    format!(
        "{} in · {} out{cost}",
        thousands(u.input_tokens),
        thousands(u.output_tokens)
    )
}

/// 15968 → "15.9k".
#[allow(clippy::cast_precision_loss, reason = "rounded for display")]
fn thousands(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1_000.),
        _ => format!("{:.1}M", n as f64 / 1_000_000.),
    }
}

#[cfg(test)]
mod tests {
    use leon_domain::{ConversationId, Entry, TimedEntry};

    use super::last_mine;

    /// An unread conversation opens at the user's last message, also when
    /// another agent wrote after it (CONV-15).
    #[test]
    fn the_last_message_of_the_user_is_found() {
        let at = |entry| TimedEntry { at: 0, entry };
        let you = |text: &str| at(Entry::User { text: text.into() });
        let agent = |text: &str| at(Entry::Agent { text: text.into() });
        let from = at(Entry::FromAgent {
            from: ConversationId(2),
            text: "Done".into(),
            answer: true,
        });
        let cases: [(Vec<TimedEntry>, Option<usize>); 3] = [
            (vec![], None),
            (vec![agent("Hi")], None),
            (
                vec![
                    you("Plan it"),
                    agent("Plan"),
                    you("Go"),
                    from,
                    agent("Done"),
                ],
                Some(2),
            ),
        ];
        for (entries, expected) in cases {
            assert_eq!(last_mine(&entries), expected);
        }
    }
}
