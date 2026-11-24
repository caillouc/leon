//! Undoing a to-do's tick or removal (TODO-1): either happens at once, and
//! a note at the window's bottom right offers "Undo" for a few seconds.

use std::time::Duration;

use gpui::{AnyElement, Context, IntoElement, prelude::*};
use leon_core::Intent;
use leon_domain::{Todo, TodoId};

use crate::workspace::Workspace;

/// How long the note offers to undo.
const SHOWN: Duration = Duration::from_secs(6);

/// What the user did to a to-do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Did {
    /// Ticked it: it went to the done ones.
    Done,
    /// Clicked its bin: it went for good.
    Removed,
}

impl Did {
    /// The note's words.
    fn said(self) -> &'static str {
        match self {
            Self::Done => "To-do done",
            Self::Removed => "To-do removed",
        }
    }
}

/// The latest tick or removal, undoable while its note shows.
#[derive(Debug, Default)]
pub(crate) struct TodoUndo {
    /// The to-do as it was, what the user did, and the action's number: a
    /// newer one takes the note's place.
    last: Option<(Todo, Did, u64)>,
    actions: u64,
}

impl Workspace {
    /// Ticks or removes to-do `id` at once, and offers to undo it.
    pub(crate) fn do_todo(&mut self, id: TodoId, did: Did, cx: &mut Context<Self>) {
        let Some(todo) = self.snapshot.todos.iter().find(|t| t.id == id).cloned() else {
            return;
        };
        self.core.send(match did {
            Did::Done => Intent::SetTodoDone(id, true),
            Did::Removed => Intent::RemoveTodo(id),
        });
        let undo = &mut self.todos.undo;
        undo.actions += 1;
        let action = undo.actions;
        undo.last = Some((todo, did, action));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SHOWN).await;
            this.update(cx, |this, cx| {
                let undo = &mut this.todos.undo;
                if undo.last.as_ref().is_some_and(|(_, _, a)| *a == action) {
                    undo.last = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// The note offering to undo the latest tick or removal: what was
    /// done, then "Undo", which puts the to-do back as it was.
    pub(crate) fn render_todo_undo(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (todo, did, _) = self.todos.undo.last.clone()?;
        Some(
            crate::flash::toast()
                .bottom_4()
                .right_4()
                .flex()
                .items_center()
                .gap_3()
                .child(did.said())
                .child(
                    crate::widgets::text_action(
                        "todo-undo",
                        "Undo",
                        cx.listener(move |this, _, _, cx| {
                            this.core.send(Intent::RestoreTodo(todo.clone()));
                            this.todos.undo.last = None;
                            cx.notify();
                        }),
                    )
                    .text_size(crate::theme::rem(12.5))
                    .text_color(crate::theme::accent()),
                )
                .into_any_element(),
        )
    }
}
