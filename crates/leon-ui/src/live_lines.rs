//! What a conversation is doing this moment, above its message box: the
//! commands it runs and for how long, its subagents, the agents it waits
//! for — so a quiet agent is never a mystery.

use gpui::{Div, SharedString, div, prelude::*};
use leon_core::ConversationView;
use leon_domain::{RunningTool, Status, running_tools};

use crate::left::ago;
use crate::spinner::{spinner, status_mark};
use crate::theme;
use crate::workspace::Workspace;

impl Workspace {
    /// What a conversation is doing this moment, a line each, with the
    /// loader: the commands and edits it runs and for how long, its
    /// subagents, or thinking, then the agents it waits for.
    pub(crate) fn live_lines(&self, s: &ConversationView) -> Vec<Div> {
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
            .map(|(ix, tool)| running_line(&format!("spin-live-tool-{id}-{ix}"), tool))
            .collect();
        if running > 0 {
            lines.push(activity(
                &format!("spin-live-sub-{id}"),
                format!(
                    "{running} subagent{} running · {started} started",
                    if running == 1 { "" } else { "s" }
                ),
            ));
        }
        if thinking {
            lines.push(activity(
                &format!("spin-live-think-{id}"),
                "Thinking…".to_owned(),
            ));
        }
        lines.extend(self.waiting_lines(s));
        lines
    }

    /// The agents that owe conversation `s` an answer, a line each with
    /// their own state (FLOW-1, CONV-16).
    pub(crate) fn waiting_lines(&self, s: &ConversationView) -> Vec<Div> {
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
                            "spin-live-wait-{}-{}",
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

/// A line of activity, with the loader.
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
