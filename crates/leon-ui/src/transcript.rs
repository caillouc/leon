//! An agent session's transcript, virtualised (only visible entries are
//! laid out), following the end while the agent writes.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    AnyElement, Context, ElementId, IntoElement, ListOffset, SharedString, div, list, prelude::*,
    px,
};
use leon_core::TranscriptView;
use leon_domain::{AgentSessionId, Entry, TimedEntry};

use crate::icons::chevron;
use crate::markdown::markdown;
use crate::theme;
use crate::widgets::empty;
use crate::workspace::Workspace;

impl Workspace {
    pub(crate) fn render_transcript(
        &mut self,
        session: AgentSessionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(transcript) = self.snapshot.transcripts.get(&session).cloned() else {
            return div().flex_1().child(empty("Loading…")).into_any_element();
        };
        let streaming = transcript.partial.is_some();
        let count = transcript.entries.len() + usize::from(streaming);
        if count == 0 {
            return div()
                .flex_1()
                .child(empty("No messages yet."))
                .into_any_element();
        }
        let state = self.list_for(session, count, streaming);
        let view = cx.entity().downgrade();
        let names: Arc<HashMap<AgentSessionId, String>> = Arc::new(
            self.snapshot
                .sessions
                .values()
                .map(|s| (s.session.id, self.session_name(s)))
                .collect(),
        );
        let expanded = self.expanded.clone();
        // Back to the user's last message once an answer follows it, to read
        // the answer from its start.
        let jump = transcript
            .entries
            .iter()
            .rposition(|e| matches!(e.entry, Entry::User { .. }))
            .filter(|&ix| ix + 1 < count)
            .map(|item_ix| {
                let state = state.clone();
                div()
                    .id("jump-to-mine")
                    .absolute()
                    .bottom(px(12.))
                    .right(px(20.))
                    .px_3()
                    .py(px(5.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::border_strong())
                    .bg(theme::surface())
                    .shadow_md()
                    .text_size(px(12.))
                    .text_color(theme::fg_muted())
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme::fg()).bg(theme::surface_2()))
                    .child("↑ Your last message")
                    .on_click(move |_, _, _| {
                        state.scroll_to(ListOffset {
                            item_ix,
                            offset_in_item: px(0.),
                        });
                    })
            });
        let list = list(state, move |ix, _window, _cx| {
            let item = match transcript.entries.get(ix) {
                Some(entry) => entry_view(
                    session,
                    ix,
                    entry,
                    expanded.contains(&(session, ix)),
                    &view,
                    &names,
                ),
                None => partial_view(session, &transcript),
            };
            // A list item is laid out alone, so `mx_auto` cannot centre it:
            // a full-width row centres the column instead.
            div()
                .w_full()
                .flex()
                .justify_center()
                .child(
                    div()
                        .w_full()
                        .max_w(px(theme::COLUMN))
                        .px_6()
                        .py_1()
                        .child(item),
                )
                .into_any_element()
        })
        .flex_1()
        .min_h_0()
        .py_3();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(list)
            .children(jump)
            .into_any_element()
    }
}

fn partial_view(session: AgentSessionId, t: &Arc<TranscriptView>) -> AnyElement {
    let text = t.partial.clone().unwrap_or_default();
    div()
        .child(markdown(
            ElementId::Name(format!("partial-{}", session.0).into()),
            &text,
        ))
        .into_any_element()
}

fn entry_view(
    session: AgentSessionId,
    ix: usize,
    timed: &TimedEntry,
    expanded: bool,
    view: &gpui::WeakEntity<Workspace>,
    names: &HashMap<AgentSessionId, String>,
) -> AnyElement {
    let id = |kind: &str| ElementId::Name(format!("{kind}-{}-{ix}", session.0).into());
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
                .py(px(9.))
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
                        .text_size(px(11.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme::accent())
                        .child("You"),
                )
                .child(long_text(session, ix, id("user"), text, expanded, view))
                .child(
                    div()
                        .id(id("edit"))
                        .absolute()
                        .top(px(6.))
                        .right(px(8.))
                        .px_2()
                        .py(px(2.))
                        .rounded_md()
                        .bg(theme::surface_2())
                        .text_size(px(11.5))
                        .text_color(theme::fg_muted())
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(1.))
                        .cursor_pointer()
                        .child("Edit from here")
                        .on_click(move |_, window, cx| {
                            let text = text_edit.clone();
                            let _ = view_edit.update(cx, |this, cx| {
                                this.start_edit(session, ix, &text, window, cx);
                            });
                        }),
                )
                .into_any_element()
        }
        Entry::FromAgent { from, text, answer } => {
            let name = names
                .get(from)
                .cloned()
                .unwrap_or_else(|| format!("session {}", from.0));
            let what = if *answer {
                "Answer from"
            } else {
                "Message from"
            };
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
                        .text_size(px(11.5))
                        .text_color(theme::accent())
                        .child(format!("{what} {name}")),
                )
                .child(long_text(
                    session,
                    ix,
                    id("from-agent"),
                    text,
                    expanded,
                    view,
                ))
                .into_any_element()
        }
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
                .text_size(px(12.))
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
                        if !this.expanded.remove(&(session, ix)) {
                            this.expanded.insert((session, ix));
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
                        .text_size(px(11.5))
                        .child(text),
                );
            }
            tool.into_any_element()
        }
        Entry::Notice { text } => div()
            .my_1()
            .flex()
            .justify_center()
            .text_size(px(12.))
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
            .text_size(px(12.5))
            .child(text.clone())
            .into_any_element(),
        Entry::TurnEnd { usage } => {
            let text = usage.map_or_default(usage_text);
            div()
                .h(px(14.))
                .flex()
                .justify_end()
                .text_size(px(10.5))
                .text_color(theme::fg_faint())
                .child(text)
                .into_any_element()
        }
    }
}

/// A message longer than [`FOLD_LINES`] shows its first lines and a toggle
/// (CONV-12).
const FOLD_LINES: usize = 12;

fn long_text(
    session: AgentSessionId,
    ix: usize,
    id: ElementId,
    text: &str,
    expanded: bool,
    view: &gpui::WeakEntity<Workspace>,
) -> AnyElement {
    let lines = text.lines().count();
    if lines <= FOLD_LINES {
        return markdown(id, text);
    }
    let shown = if expanded {
        text.to_owned()
    } else {
        text.lines().take(6).collect::<Vec<_>>().join("\n")
    };
    let view = view.clone();
    div()
        .child(markdown(id, &shown))
        .child(
            div()
                .id(("fold", ix))
                .mt_1()
                .text_size(px(12.))
                .text_color(theme::accent())
                .cursor_pointer()
                .child(if expanded {
                    "Show less".to_owned()
                } else {
                    format!("Show all · {lines} lines")
                })
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        if !this.expanded.remove(&(session, ix)) {
                            this.expanded.insert((session, ix));
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
