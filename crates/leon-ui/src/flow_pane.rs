//! Main pane for a flow: header (title, branch), tabs (the flow and
//! each conversation), transcript and composer.

use gpui::{AnyElement, Context, IntoElement, SharedString, Window, div, prelude::*};
use leon_core::{ConversationView, Intent};
use leon_domain::{ConversationId, FlowId, Status};

use crate::flow_tabs::FlowTabs;
use crate::icons::{Icon, icon};
use crate::spinner::status_mark;
use crate::theme;
use crate::widgets::{HEADER_HEIGHT, empty};
use crate::workspace::{Tab, Workspace};

impl Workspace {
    pub(crate) fn render_flow_pane(
        &mut self,
        id: FlowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(view) = self.snapshot.flow(id).cloned() else {
            return empty("This flow no longer exists.");
        };
        let conversation = self.current_conversation();
        let body = match conversation {
            None if self.tabs.get(&id) == Some(&Tab::New) => {
                self.render_new_flow(Some(id), window, cx)
            }
            None if view.conversations.is_empty() => empty("No conversation yet."),
            None => self.render_flow(id, window, cx),
            Some(conversation) => {
                let (transcript, jump) = self.render_transcript(conversation, cx);
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(transcript)
                    .child(crate::widgets::centered(
                        theme::COLUMN,
                        self.render_composer(conversation, jump, window, cx),
                    ))
                    .into_any_element()
            }
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.render_header(&view, window, cx))
            .child(body)
            .into_any_element()
    }

    fn render_header(
        &mut self,
        view: &leon_core::FlowView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = view.flow.id;
        let c = &view.flow;
        let title = div()
            .text_size(crate::theme::rem(15.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(c.title.clone());
        let current = self.current_conversation();
        let adding = self.tabs.get(&id) == Some(&Tab::New);
        // More tabs than the pane is wide scroll sideways, with a trackpad
        // swipe or the mouse wheel.
        let mut tabs = div()
            .id(SharedString::from(format!("flow-tabs-{}", id.0)))
            .flex()
            .gap_1()
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll)
            .child(Self::tab_button(
                "tab-flow",
                "Flow".into(),
                // An answer not read yet waits on the diagram: a green dot
                // (NAV-4).
                view.flow.unread.then(|| {
                    div()
                        .size(crate::theme::rem(7.))
                        .rounded_full()
                        .bg(theme::ok())
                        .into_any_element()
                }),
                current.is_none() && !adding,
                id,
                Tab::Flow,
                cx,
            ));
        for sid in self.agent_tabs(id, &view.conversations) {
            if let Some(s) = self.snapshot.conversations.get(&sid) {
                let selected = current == Some(sid);
                let name = format!("tab-{}", sid.0);
                // Its name alone: the CLI shows in its message box.
                let tab = Self::tab_button(
                    &name,
                    s.name.clone().into(),
                    Some(status_mark(
                        SharedString::from(format!("spin-tab-{}", sid.0)),
                        s.status,
                    )),
                    selected,
                    id,
                    Tab::Conversation(sid),
                    cx,
                );
                tabs = tabs.child(tab.child(close_tab(id, sid, name.into(), selected, cx)));
            }
        }
        let _ = window;
        // One click to come back to it later (NAV-4): the conversation shown,
        // else the flow; not when it already is.
        let unread = match current {
            Some(sid) => {
                let read = self
                    .snapshot
                    .conversations
                    .get(&sid)
                    .is_some_and(|s| !s.conversation.unread);
                read.then_some(Intent::MarkConversationUnread(sid))
            }
            None => (!view.flow.unread).then_some(Intent::MarkUnread(id)),
        };
        let mark_unread = unread.map(|intent| {
            crate::widgets::text_action(
                "mark-unread",
                "Mark as unread",
                cx.listener(move |this, _, _, _| this.core.send(intent.clone())),
            )
        });
        div()
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(crate::theme::rem(HEADER_HEIGHT))
                    .px_5()
                    // The top-right buttons' room: to-dos, and the right
                    // pane's while it is closed.
                    .pr(crate::theme::rem(
                        crate::todos_view::BUTTON_ROOM
                            + if self.panes.right_open { 0. } else { 36. },
                    ))
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(div().flex_1().min_w_0().child(title))
                    .children(mark_unread),
            )
            .child(
                div()
                    .px_4()
                    .pt_2()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(tabs),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn tab_button(
        id: &str,
        text: SharedString,
        mark: Option<AnyElement>,
        selected: bool,
        flow: FlowId,
        tab: Tab,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(SharedString::from(id.to_owned()))
            .group(SharedString::from(id.to_owned()))
            .flex_none()
            .whitespace_nowrap()
            .px_2()
            .pb_2()
            .pt_1()
            .flex()
            .items_center()
            .gap_2()
            .border_b_2()
            .border_color(if selected {
                theme::accent()
            } else {
                gpui::transparent_black().into()
            })
            .text_color(if selected {
                theme::fg()
            } else {
                theme::fg_muted()
            })
            .cursor_pointer()
            .hover(|s| s.text_color(theme::fg()))
            .children(mark)
            .child(text)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.tabs.insert(flow, tab);
                cx.notify();
            }))
    }

    /// The agent tabs of flow `flow` after Flow (FLOW-3): those opened and
    /// those the user talks with, less those they closed.
    fn agent_tabs(&self, flow: FlowId, all: &[ConversationId]) -> Vec<ConversationId> {
        let listed = |c: ConversationId| {
            let Some(s) = self.snapshot.conversations.get(&c) else {
                return false;
            };
            !s.conversation.tab_closed
                && (s.conversation.started_by.is_none()
                    || self
                        .snapshot
                        .transcripts
                        .get(&c)
                        .is_some_and(|t| leon_domain::talks_with_user(&t.entries)))
        };
        let none = FlowTabs::default();
        self.flow_tabs
            .get(&flow)
            .unwrap_or(&none)
            .order(all, listed)
    }

    pub(crate) fn status_line(s: &ConversationView) -> Option<String> {
        let (started, running) = s.subagents;
        let sub = (started > 0).then(|| format!("Subagents: {started} started, {running} running"));
        let state = match s.status {
            Status::Running => Some("Working…".to_owned()),
            Status::NeedsInput => Some("Waiting for your answer".to_owned()),
            Status::Error => Some("The last turn failed. Send a message to try again.".to_owned()),
            Status::Idle => None,
        };
        match (state, sub) {
            (Some(a), Some(b)) => Some(format!("{a} · {b}")),
            (a, b) => a.or(b),
        }
    }
}

/// The cross closing a conversation's tab (FLOW-3): on the tab shown, and
/// on the others when hovered. The conversation stays in the flow; the
/// Flow tab shows when it was the one shown. Closed stays closed across
/// restarts, until the conversation is opened again.
fn close_tab(
    flow: FlowId,
    conversation: ConversationId,
    group: SharedString,
    shown: bool,
    cx: &mut Context<Workspace>,
) -> AnyElement {
    div()
        .id(SharedString::from(format!("tab-close-{}", conversation.0)))
        .flex_none()
        .rounded(crate::theme::rem(4.))
        .p(crate::theme::rem(2.))
        .cursor_pointer()
        .when(!shown, |d| {
            d.opacity(0.).group_hover(group, |s| s.opacity(1.))
        })
        .hover(|s| s.bg(theme::surface_2()))
        .child(icon(Icon::Close, theme::fg_faint(), 12.))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.flow_tabs.entry(flow).or_default().close(conversation);
            this.core.send(Intent::CloseTab(conversation));
            if this.tabs.get(&flow) == Some(&Tab::Conversation(conversation)) {
                this.tabs.insert(flow, Tab::Flow);
            }
            cx.notify();
        }))
        .into_any_element()
}
