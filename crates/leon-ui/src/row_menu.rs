//! Right-click menus: a flow's in the left pane (NAV-3, NAV-4, NAV-8:
//! rename in place, pin, mark as unread, archive, delete), a project's
//! (remove it from Leon), and a conversation's on its Flow box (NAV-4: mark
//! it as unread). Deleting and removing ask first.

use gpui::{
    AnyElement, Context, IntoElement, PromptLevel, Stateful, Window, anchored, deferred, div,
    prelude::*,
};
use gpui_component::input::{InputEvent, InputState};
use leon_core::Intent;
use leon_domain::{ConversationId, FlowId, ProjectId};

use crate::theme;
use crate::workspace::{Workspace, value_of};

/// What a right-click menu is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuFor {
    Flow(FlowId),
    Project(ProjectId),
    Conversation(ConversationId),
}

impl Workspace {
    /// The open right-click menu, drawn above everything at the click.
    pub(crate) fn render_row_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (target, at) = self.row_menu?;
        let items = match target {
            MenuFor::Flow(id) => self.flow_items(id, cx)?,
            MenuFor::Project(id) => vec![menu_item(
                "remove-project",
                "Remove project…",
                true,
                cx,
                move |this, window, cx| this.confirm_remove_project(id, window, cx),
            )],
            MenuFor::Conversation(id) => vec![menu_item(
                "unread-conversation",
                "Mark as unread",
                false,
                cx,
                move |this, _, _| this.core.send(Intent::MarkConversationUnread(id)),
            )],
        };
        let menu = div()
            .id("row-menu")
            .min_w(crate::theme::rem(180.))
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::surface())
            .shadow_lg()
            .children(items)
            // A pick does not also click the row under the menu.
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.row_menu = None;
                cx.notify();
            }));
        Some(
            deferred(
                anchored()
                    .position(at)
                    .snap_to_window_with_margin(gpui::px(8.))
                    .child(menu),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    fn flow_items(&self, id: FlowId, cx: &mut Context<Self>) -> Option<Vec<AnyElement>> {
        let c = &self.snapshot.flow(id)?.flow;
        let (pinned, archived, unread) = (c.pinned, c.archived, c.unread);
        let mut items = vec![
            menu_item("rename", "Rename", false, cx, move |this, window, cx| {
                this.start_rename(id, window, cx);
            }),
            menu_item(
                "pin",
                if pinned { "Unpin" } else { "Pin to top" },
                false,
                cx,
                move |this, _, _| this.core.send(Intent::Pin(id, !pinned)),
            ),
        ];
        if !unread {
            items.push(menu_item(
                "unread",
                "Mark as unread",
                false,
                cx,
                move |this, _, _| this.core.send(Intent::MarkUnread(id)),
            ));
        }
        items.push(menu_item(
            "archive",
            if archived { "Unarchive" } else { "Archive" },
            false,
            cx,
            move |this, _, _| this.core.send(Intent::Archive(id, !archived)),
        ));
        items.push(
            div()
                .my_1()
                .h(crate::theme::rem(1.))
                .bg(theme::border())
                .into_any_element(),
        );
        items.push(menu_item(
            "delete",
            "Delete…",
            true,
            cx,
            move |this, window, cx| {
                this.confirm_delete(id, window, cx);
            },
        ));
        Some(items)
    }

    /// Turns the row's title into an input; Enter saves.
    fn start_rename(&mut self, id: FlowId, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .snapshot
            .flow(id)
            .map_or_default(|c| c.flow.title.clone());
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

    fn confirm_delete(&mut self, id: FlowId, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .snapshot
            .flow(id)
            .map_or_default(|c| c.flow.title.clone());
        self.confirm(
            &format!("Delete \u{201c}{title}\u{201d}?"),
            "Its transcripts are removed from Leon. Files and the CLIs' own sessions stay.",
            "Delete",
            Intent::Delete(id),
            window,
            cx,
        );
    }

    fn confirm_remove_project(
        &mut self,
        id: ProjectId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.snapshot.projects.iter().find(|p| p.id == id) else {
            return;
        };
        let count = self
            .snapshot
            .flows
            .iter()
            .filter(|c| c.flow.project == id)
            .count();
        let flows = match count {
            0 => String::new(),
            1 => "Its flow is deleted from Leon. ".to_owned(),
            n => format!("Its {n} flows are deleted from Leon. "),
        };
        self.confirm(
            &format!("Remove \u{201c}{}\u{201d} from Leon?", project.name),
            &format!(
                "{flows}The folder {} and the CLIs' own sessions stay.",
                project.path.display()
            ),
            "Remove",
            Intent::RemoveProject(id),
            window,
            cx,
        );
    }

    /// Asks with the system's own dialog, then sends `intent` on `yes`.
    fn confirm(
        &self,
        question: &str,
        detail: &str,
        yes: &str,
        intent: Intent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = window.prompt(
            PromptLevel::Warning,
            question,
            Some(detail),
            &[yes, "Cancel"],
            cx,
        );
        let core = self.core.clone();
        cx.spawn(async move |_, _| {
            if answer.await == Ok(0) {
                core.send(intent);
            }
        })
        .detach();
    }
}

/// One entry of a right-click menu: closes the menu, then runs `on_pick`.
fn menu_item(
    key: &'static str,
    text: &'static str,
    danger: bool,
    cx: &mut Context<Workspace>,
    on_pick: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> AnyElement {
    let item: Stateful<gpui::Div> = div()
        .id(key)
        .px_3()
        .py(crate::theme::rem(5.))
        .rounded_md()
        .text_size(crate::theme::rem(12.5))
        .when(danger, |d| d.text_color(theme::err()))
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_2()))
        .child(text);
    item.on_click(cx.listener(move |this, _, window, cx| {
        this.row_menu = None;
        on_pick(this, window, cx);
        cx.notify();
    }))
    .into_any_element()
}
