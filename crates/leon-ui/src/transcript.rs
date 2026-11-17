//! An agent session's transcript, virtualised (only visible entries are
//! laid out), following the end while the agent writes.

use std::sync::Arc;

use gpui::{AnyElement, Context, ElementId, IntoElement, SharedString, div, list, prelude::*, px};
use gpui_component::text::TextView;
use leon_core::TranscriptView;
use leon_domain::{AgentSessionId, Entry, TimedEntry};

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
        let expanded = self.expanded_tools.clone();
        list(state, move |ix, _window, cx| {
            let item = match transcript.entries.get(ix) {
                Some(entry) => entry_view(
                    session,
                    ix,
                    entry,
                    expanded.contains(&(session, ix)),
                    &view,
                    cx,
                ),
                None => partial_view(session, &transcript),
            };
            div()
                .px_6()
                .py_1()
                .max_w(px(860.))
                .w_full()
                .mx_auto()
                .child(item)
                .into_any_element()
        })
        .flex_1()
        .min_h_0()
        .py_3()
        .into_any_element()
    }
}

fn markdown(id: ElementId, text: &str) -> AnyElement {
    TextView::markdown(id, SharedString::from(text.to_owned()))
        .selectable(true)
        .into_any_element()
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
    _cx: &mut gpui::App,
) -> AnyElement {
    let id = |kind: &str| ElementId::Name(format!("{kind}-{}-{ix}", session.0).into());
    match &timed.entry {
        Entry::User { text } => div()
            .my_1()
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .child(markdown(id("user"), text))
            .into_any_element(),
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
                        .child(
                            div()
                                .text_color(theme::fg())
                                .child(format!("{} {name}", if expanded { "▾" } else { "▸" })),
                        )
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
                        if !this.expanded_tools.remove(&(session, ix)) {
                            this.expanded_tools.insert((session, ix));
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
                        .font_family("Menlo")
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
            let text = usage.map_or_default(|u| {
                let cost = u.cost_usd.map_or_default(|c| format!(" · ${c:.2}"));
                format!(
                    "{} in · {} out{cost}",
                    thousands(u.input_tokens),
                    thousands(u.output_tokens)
                )
            });
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

/// 15968 → "15.9k".
#[allow(clippy::cast_precision_loss, reason = "rounded for display")]
fn thousands(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1_000.),
        _ => format!("{:.1}M", n as f64 / 1_000_000.),
    }
}
