//! The main window: conversations (left), conversation (main), context
//! (right). M0 shows the panes and the project list from the store.

use std::sync::Arc;
use std::time::Instant;

use gpui::{AnyElement, Context, IntoElement, Render, Task, Window, div, prelude::*, px, rgb};
use leon_core::{CoreHandle, Snapshot};

use crate::theme;

const LEFT_WIDTH: f32 = 268.;
const RIGHT_WIDTH: f32 = 420.;

/// Root view of the window.
pub struct Workspace {
    snapshot: Arc<Snapshot>,
    started: Option<Instant>,
    _updates: Task<()>,
}

impl Workspace {
    /// Builds the view and follows the core's snapshots.
    pub fn new(core: &CoreHandle, started: Instant, cx: &mut Context<Self>) -> Self {
        let mut snapshots = core.subscribe();
        let updates = cx.spawn(async move |this, cx| {
            while snapshots.changed().await.is_ok() {
                let next = Arc::clone(&snapshots.borrow_and_update());
                let alive = this.update(cx, |view, cx| {
                    view.snapshot = next;
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            snapshot: core.snapshot(),
            started: Some(started),
            _updates: updates,
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(started) = self.started.take() {
            window.on_next_frame(move |_window, _cx| {
                tracing::info!(startup_ms = started.elapsed().as_millis(), "first frame");
            });
        }
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(rgb(theme::WINDOW))
            .text_color(rgb(theme::FG))
            .text_size(px(13.))
            .child(left_pane(&self.snapshot))
            .child(main_pane())
            .child(right_pane())
    }
}

fn left_pane(snapshot: &Snapshot) -> impl IntoElement {
    let body: AnyElement = if !snapshot.loaded {
        empty_state("Loading…")
    } else if snapshot.projects.is_empty() {
        empty_state("No projects yet.")
    } else {
        div()
            .flex()
            .flex_col()
            .px_2()
            .children(
                snapshot
                    .projects
                    .iter()
                    .map(|project| div().px_2().py_1().rounded_md().child(project.name.clone())),
            )
            .into_any_element()
    };
    div()
        .w(px(LEFT_WIDTH))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .bg(rgb(theme::PANEL))
        .border_r_1()
        .border_color(rgb(theme::BORDER))
        .child(section_label("Projects"))
        .child(body)
}

fn main_pane() -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(theme::FG_MUTED))
        .child("Select a conversation")
}

fn right_pane() -> impl IntoElement {
    div()
        .w(px(RIGHT_WIDTH))
        .h_full()
        .flex_none()
        .bg(rgb(theme::PANEL))
        .border_l_1()
        .border_color(rgb(theme::BORDER))
        .child(empty_state(
            "Changes, branch diff and pull request show here.",
        ))
}

fn section_label(text: &'static str) -> impl IntoElement {
    div()
        .px_4()
        .pt_3()
        .pb_1()
        .text_size(px(11.))
        .text_color(rgb(theme::FG_FAINT))
        .child(text.to_uppercase())
}

fn empty_state(text: &'static str) -> AnyElement {
    div()
        .px_4()
        .py_2()
        .text_color(rgb(theme::FG_FAINT))
        .child(text)
        .into_any_element()
}
