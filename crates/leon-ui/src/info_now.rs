//! The Info tab's "Now" (CTX-1): what each agent of the flow is doing
//! this moment — which commands it runs and for how long, its subagents, the
//! answer it waits for — so a quiet agent is never a mystery.

use gpui::{Div, FontWeight, SharedString, div, prelude::*};
use leon_core::ConversationView;
use leon_domain::{RunningTool, Status, running_tools};

use crate::left::ago;
use crate::spinner::{spinner, status_mark};
use crate::theme;
use crate::workspace::Workspace;

/// Key of "Now" folded, for every flow.
const NOW: &str = "info:now";

impl Workspace {
    /// One block per conversation: its state, then what it is running;
    /// folds to its title.
    pub(crate) fn now_card(&self, conversations: &[&ConversationView]) -> Div {
        let folded = self.snapshot.folded_groups.contains(NOW);
        div()
            .mx_3()
            .px_3()
            .py(crate::theme::rem(10.))
            .flex()
            .flex_col()
            .gap_3()
            .rounded_lg()
            .border_1()
            .border_color(theme::border())
            .bg(theme::window())
            .child(
                crate::widgets::fold_toggle(NOW.to_owned(), folded).child(
                    div()
                        .text_size(crate::theme::rem(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Now"),
                ),
            )
            .when(!folded, |d| {
                d.children(conversations.iter().map(|s| {
                    self.now_conversation(s, crate::agent_color::agent_color(conversations, s))
                }))
            })
    }

    /// One agent, in its colour from the flow diagram: a band on its left,
    /// its name, its CLI and model.
    fn now_conversation(&self, s: &ConversationView, agent: gpui::Rgba) -> Div {
        let id = s.conversation.id.0;
        let (state, color) = state_word(s);
        let (model, _) = self.model_and_effort(&s.conversation.settings);
        div()
            .flex()
            .flex_col()
            .gap(crate::theme::rem(4.))
            .pl_2()
            .border_l_2()
            .border_color(agent)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(crate::theme::rem(12.5))
                    .child(status_mark(
                        SharedString::from(format!("spin-now-{id}")),
                        s.status,
                    ))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(agent)
                            .child(s.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .px(crate::theme::rem(5.))
                            .rounded(crate::theme::rem(4.))
                            .bg(theme::surface_2())
                            .text_size(crate::theme::rem(10.5))
                            .text_color(theme::fg_muted())
                            .child(format!(
                                "{} · {model}",
                                s.conversation.settings.backend.display_name()
                            )),
                    )
                    // A narrow pane cuts it short rather than overflow.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(color)
                            .child(state),
                    ),
            )
            .children(
                self.live_lines("now", s)
                    .into_iter()
                    .map(|line| line.pl(crate::theme::rem(20.))),
            )
            .children(s.question.as_ref().map(|q| {
                div()
                    .pl(crate::theme::rem(20.))
                    .text_size(crate::theme::rem(12.))
                    .text_color(theme::warn())
                    .child(format!("“{}”", q.title))
            }))
    }
}

impl Workspace {
    /// What a conversation is doing this moment, a line each, with the
    /// turning loader: the commands and edits it runs and for how long, its
    /// subagents, or thinking, then the agents it waits for. `place` keeps
    /// the loaders' ids apart where it shows twice (Info, the message box).
    pub(crate) fn live_lines(&self, place: &str, s: &ConversationView) -> Vec<Div> {
        let id = s.conversation.id.0;
        // Calls without a result mean something only while a turn runs.
        let live = matches!(s.status, Status::Running | Status::NeedsInput);
        let tools: Vec<RunningTool> = self
            .snapshot
            .transcripts
            .get(&s.conversation.id)
            .filter(|_| live)
            .map_or_default(|t| running_tools(&t.entries));
        let (started, running) = s.subagents;
        let thinking = s.status == Status::Running && tools.is_empty() && running == 0;
        let mut lines: Vec<Div> = tools
            .iter()
            .enumerate()
            .map(|(ix, tool)| running_line(&format!("spin-{place}-tool-{id}-{ix}"), tool))
            .collect();
        if running > 0 {
            lines.push(activity(
                &format!("spin-{place}-sub-{id}"),
                format!(
                    "{running} subagent{} running · {started} started",
                    if running == 1 { "" } else { "s" }
                ),
            ));
        }
        if thinking {
            lines.push(activity(
                &format!("spin-{place}-think-{id}"),
                "Thinking…".to_owned(),
            ));
        }
        lines.extend(self.waiting_lines(place, s));
        lines
    }

    /// The agents that owe conversation `s` an answer, a line each with
    /// their own state (FLOW-1, CONV-16).
    pub(crate) fn waiting_lines(&self, place: &str, s: &ConversationView) -> Vec<Div> {
        self.waited(s)
            .map(|other| {
                let (state, _) = state_word(other);
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(crate::theme::rem(12.))
                    .text_color(theme::fg_muted())
                    .child(status_mark(
                        SharedString::from(format!(
                            "spin-{place}-wait-{}-{}",
                            s.conversation.id.0, other.conversation.id.0
                        )),
                        other.status,
                    ))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(format!("Waiting for {} · {state}", other.name.clone())),
                    )
            })
            .collect()
    }

    /// The conversations that owe conversation `s` an answer.
    pub(crate) fn waited<'a>(
        &'a self,
        s: &'a ConversationView,
    ) -> impl Iterator<Item = &'a ConversationView> + 'a {
        s.waiting_for
            .iter()
            .filter_map(|id| self.snapshot.conversations.get(id))
    }
}

/// A conversation's state in a word or two, and its colour.
fn state_word(s: &ConversationView) -> (&'static str, gpui::Rgba) {
    match s.status {
        Status::Running => ("Working", theme::accent()),
        Status::NeedsInput => ("Waiting for your answer", theme::warn()),
        Status::Error => ("Stopped with an error", theme::err()),
        Status::Idle if !s.waiting_for.is_empty() => ("Waiting", theme::fg_muted()),
        Status::Idle => ("Idle", theme::fg_muted()),
    }
}

/// A command or edit still running, with how long it has run.
fn running_line(key: &str, tool: &RunningTool) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_size(crate::theme::rem(12.))
        .child(spinner(
            SharedString::from(key.to_owned()),
            theme::accent(),
            11.,
        ))
        .child(div().flex_none().child(tool.name.clone()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(theme::MONO)
                .text_size(crate::theme::rem(11.5))
                .text_color(theme::fg_muted())
                .child(tool.detail.clone()),
        )
        .child(
            div()
                .flex_none()
                .text_color(theme::fg_faint())
                .child(ago(tool.since)),
        )
}

/// A line of activity, with the turning loader.
fn activity(key: &str, text: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_size(crate::theme::rem(12.))
        .text_color(theme::fg_muted())
        .child(spinner(
            SharedString::from(key.to_owned()),
            theme::accent(),
            11.,
        ))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text),
        )
}
