//! The Info tab's "Now" (CTX-1): what each agent of the conversation is doing
//! this moment — which commands it runs and for how long, its subagents, the
//! answer it waits for — so a quiet agent is never a mystery.

use gpui::{Div, FontWeight, SharedString, div, prelude::*, px};
use leon_core::SessionView;
use leon_domain::{RunningTool, Status, running_tools};

use crate::left::ago;
use crate::spinner::{spinner, status_mark};
use crate::theme;
use crate::workspace::Workspace;

impl Workspace {
    /// One block per agent session: its state, then what it is running.
    pub(crate) fn now_card(&self, sessions: &[&SessionView]) -> Div {
        div()
            .mx_3()
            .px_3()
            .py(px(10.))
            .flex()
            .flex_col()
            .gap_3()
            .rounded_lg()
            .border_1()
            .border_color(theme::border())
            .bg(theme::window())
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Now"),
            )
            .children(sessions.iter().map(|s| self.now_session(s)))
    }

    fn now_session(&self, s: &SessionView) -> Div {
        let id = s.session.id.0;
        let (state, color) = match s.status {
            Status::Running => ("Working", theme::accent()),
            Status::NeedsInput => ("Waiting for your answer", theme::warn()),
            Status::Error => ("Stopped with an error", theme::err()),
            Status::Idle => ("Idle", theme::fg_muted()),
        };
        // Calls without a result mean something only while a turn runs.
        let live = matches!(s.status, Status::Running | Status::NeedsInput);
        let tools: Vec<RunningTool> = self
            .snapshot
            .transcripts
            .get(&s.session.id)
            .filter(|_| live)
            .map_or_default(|t| running_tools(&t.entries));
        let (started, running) = s.subagents;
        let thinking = s.status == Status::Running && tools.is_empty() && running == 0;
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.5))
                    .child(status_mark(
                        SharedString::from(format!("spin-now-{id}")),
                        s.status,
                    ))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.session_name(s)),
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
                tools
                    .into_iter()
                    .enumerate()
                    .map(|(ix, tool)| running_line(&format!("spin-tool-{id}-{ix}"), &tool)),
            )
            .when(running > 0, |d| {
                d.child(activity(
                    &format!("spin-sub-{id}"),
                    format!(
                        "{running} subagent{} running · {started} started",
                        if running == 1 { "" } else { "s" }
                    ),
                ))
            })
            .when(thinking, |d| {
                d.child(activity(
                    &format!("spin-think-{id}"),
                    "Thinking…".to_owned(),
                ))
            })
            .children(s.question.as_ref().map(|q| {
                div()
                    .pl(px(20.))
                    .text_size(px(12.))
                    .text_color(theme::warn())
                    .child(format!("“{}”", q.title))
            }))
    }
}

/// A command or edit still running, with how long it has run.
fn running_line(key: &str, tool: &RunningTool) -> Div {
    div()
        .pl(px(20.))
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(12.))
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
                .text_size(px(11.5))
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

/// A line of activity under an agent, with the turning loader.
fn activity(key: &str, text: String) -> Div {
    div()
        .pl(px(20.))
        .flex()
        .items_center()
        .gap_2()
        .text_size(px(12.))
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
