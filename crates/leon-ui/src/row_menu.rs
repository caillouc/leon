//! Right-click menu of a conversation in the left pane (NAV-3, NAV-4,
//! NAV-8): rename in place, pin, mark as unread, archive, delete after a
//! confirmation.

use gpui::{
    AnyElement, Context, IntoElement, PromptLevel, Window, anchored, deferred, div, prelude::*, px,
};
use gpui_component::input::{InputEvent, InputState};
use leon_core::Intent;
use leon_domain::ConversationId;

use crate::theme;
use crate::workspace::{Workspace, value_of};

impl Workspace {
    /// The open right-click menu, drawn above everything at the click.
    pub(crate) fn render_row_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, at) = self.row_menu?;
        let c = &self.snapshot.conversation(id)?.conversation;
        let (pinned, archived, unread) = (c.pinned, c.archived, c.unread);
        let item = |key: &'static str, text: &'static str, danger: bool| {
            div()
                .id(key)
                .px_3()
                .py(px(5.))
                .rounded_md()
                .text_size(px(12.5))
                .when(danger, |d| d.text_color(theme::err()))
                .cursor_pointer()
                .hover(|s| s.bg(theme::surface_2()))
                .child(text)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.row_menu = None;
                    match key {
                        "rename" => this.start_rename(id, window, cx),
                        "pin" => this.core.send(Intent::Pin(id, !pinned)),
                        "unread" => this.core.send(Intent::MarkUnread(id)),
                        "archive" => this.core.send(Intent::Archive(id, !archived)),
                        "delete" => this.confirm_delete(id, window, cx),
                        _ => {}
                    }
                    cx.notify();
                }))
        };
        let menu = div()
            .id("row-menu")
            .min_w(px(180.))
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::surface())
            .shadow_lg()
            .child(item("rename", "Rename", false))
            .child(item(
                "pin",
                if pinned { "Unpin" } else { "Pin to top" },
                false,
            ))
            .when(!unread, |d| {
                d.child(item("unread", "Mark as unread", false))
            })
            .child(item(
                "archive",
                if archived { "Unarchive" } else { "Archive" },
                false,
            ))
            .child(div().my_1().h(px(1.)).bg(theme::border()))
            .child(item("delete", "Delete…", true))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.row_menu = None;
                cx.notify();
            }));
        Some(
            deferred(
                anchored()
                    .position(at)
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// Turns the row's title into an input; Enter saves.
    fn start_rename(&mut self, id: ConversationId, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .snapshot
            .conversation(id)
            .map_or_default(|c| c.conversation.title.clone());
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(title, window, cx);
            state
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.finish_rename(cx);
            }
        });
        input.update(cx, |state, cx| state.focus(window, cx));
        self.rename = Some((id, input, subscription));
        cx.notify();
    }

    pub(crate) fn finish_rename(&mut self, cx: &mut Context<Self>) {
        if let Some((id, input, _)) = self.rename.take() {
            let title = value_of(&input, cx);
            if !title.trim().is_empty() {
                self.core.send(Intent::Rename(id, title.trim().to_owned()));
            }
        }
        cx.notify();
    }

    /// Asks with the system's own dialog before deleting.
    fn confirm_delete(&mut self, id: ConversationId, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .snapshot
            .conversation(id)
            .map_or_default(|c| c.conversation.title.clone());
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Delete \u{201c}{title}\u{201d}?"),
            Some("Its transcripts are removed from Leon. Files and the CLIs' own sessions stay."),
            &["Delete", "Cancel"],
            cx,
        );
        let core = self.core.clone();
        cx.spawn(async move |_, _| {
            if answer.await == Ok(0) {
                core.send(Intent::Delete(id));
            }
        })
        .detach();
    }
}
