//! The user's messages waiting for the answer to end (CONV-19): a line each
//! above the message box, with Edit and Remove. Edited, a message takes the
//! box alone — what was written there is put aside — and, sent again, goes
//! back to its place among the others, what was written coming back.

use gpui::{Context, Div, Focusable, SharedString, Window, div, prelude::*};
use gpui_component::input::InputContent;
use leon_core::ports::Image;
use leon_core::{ConversationView, Intent};
use leon_domain::ConversationId;

use crate::attach::Put;
use crate::theme;
use crate::workspace::Workspace;

/// A queued message being edited in the message box.
pub(crate) struct QueuedEdit {
    conversation: ConversationId,
    /// Its place among the waiting messages.
    at: usize,
    /// What the box held before, its pastes and images: back once sent.
    aside: (InputContent, Put),
}

impl Workspace {
    /// A line per message of `s` waiting for the answer to end: "Queued",
    /// its first line, "Edit" (none while one is edited) and "Remove".
    pub(crate) fn queued_lines(&self, s: &ConversationView, cx: &mut Context<Self>) -> Vec<Div> {
        let conversation = s.conversation.id;
        let editing = self
            .queued_edit
            .as_ref()
            .is_some_and(|e| e.conversation == conversation);
        s.queued
            .iter()
            .enumerate()
            .map(|(ix, (text, _))| {
                let first = text.lines().next().unwrap_or_default().to_owned();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(crate::theme::rem(12.))
                    .text_color(theme::fg_muted())
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme::accent())
                            .child("Queued"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(first),
                    )
                    .when(!editing, |d| {
                        d.child(crate::widgets::text_action(
                            SharedString::from(format!("edit-queued-{ix}")),
                            "Edit",
                            cx.listener(move |this, _, window, cx| {
                                this.edit_queued(conversation, ix, window, cx);
                            }),
                        ))
                    })
                    .child(crate::widgets::text_action(
                        SharedString::from(format!("unqueue-{ix}")),
                        "Remove",
                        cx.listener(move |this, _, _, _| {
                            this.core.send(Intent::Unqueue(conversation, ix));
                        }),
                    ))
            })
            .collect()
    }

    /// Queued message `ix` alone in the message box, to change it; what
    /// the box held is put aside. It leaves the queue until sent again.
    fn edit_queued(
        &mut self,
        conversation: ConversationId,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((text, images)) = self
            .snapshot
            .conversations
            .get(&conversation)
            .and_then(|s| s.queued.get(ix))
            .cloned()
        else {
            return;
        };
        let aside = (
            self.composer.read(cx).content(),
            std::mem::take(&mut self.put),
        );
        let (content, put) = crate::attach::restore(&text, &images);
        self.put.images = put;
        self.set_composer(content, window, cx);
        self.composer.focus_handle(cx).focus(window, cx);
        self.queued_edit = Some(QueuedEdit {
            conversation,
            at: ix,
            aside,
        });
        self.core.send(Intent::Unqueue(conversation, ix));
    }

    /// The message sent is a queued one edited: back to its place, and
    /// what the box held before comes back. `false` when none is edited.
    pub(crate) fn requeue(
        &mut self,
        conversation: ConversationId,
        text: String,
        images: Vec<Image>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.queued_edit.take_if(|e| e.conversation == conversation) else {
            return false;
        };
        self.core.send(Intent::Requeue {
            conversation,
            at: edit.at,
            text,
            images,
        });
        let (content, put) = edit.aside;
        self.put = put;
        self.set_composer(content, window, cx);
        true
    }

    /// The message box holds `content`, the caret at its end.
    fn set_composer(&mut self, content: InputContent, window: &mut Window, cx: &mut Context<Self>) {
        let end = content.text().len();
        self.composer.update(cx, |state, cx| {
            state.set_value(content, window, cx);
            state.set_selected_range(end..end, cx);
        });
    }
}
