//! Main pane for a flow: header (title, branch), tabs (the flow and
//! each conversation), transcript and composer.

use gpui::{
    AnyElement, ClipboardItem, Context, IntoElement, SharedString, Window, div, prelude::*, px,
};
use leon_core::ConversationView;
use leon_domain::{FlowId, Status};

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
                    .child(self.render_composer(conversation, jump, window, cx))
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
            .text_size(px(15.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(c.title.clone());
        let folder = view
            .conversations
            .first()
            .and_then(|s| self.snapshot.conversations.get(s))
            .map(|s| s.conversation.working_dir.clone());
        let branch = self.folder_git(id).and_then(|g| g.branch.clone());
        let chip_text: SharedString = branch
            .clone()
            .or_else(|| {
                folder
                    .as_ref()
                    .and_then(|f| f.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_default()
            .into();
        let copy = chip_text.clone();
        let chip = div()
            .id("worktree-chip")
            .px_2()
            .py(px(2.))
            .rounded_md()
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .text_size(px(11.5))
            .text_color(theme::fg_muted())
            .cursor_pointer()
            .hover(|s| s.text_color(theme::fg()))
            .child(format!("⎇ {chip_text}"))
            .on_click(move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string()));
            });

        let current = self.current_conversation();
        let adding = self.tabs.get(&id) == Some(&Tab::New);
        // More tabs than the pane is wide scroll sideways, with a trackpad
        // swipe or the mouse wheel.
        let mut tabs = div()
            .id(SharedString::from(format!("flow-tabs-{}", id.0)))
            .flex()
            .gap_1()
            .overflow_x_scroll()
            .child(Self::tab_button(
                "tab-flow",
                "Flow".into(),
                None,
                current.is_none() && !adding,
                id,
                Tab::Flow,
                cx,
            ));
        for sid in &view.conversations {
            if let Some(s) = self.snapshot.conversations.get(sid).cloned() {
                let selected = current == Some(*sid);
                let name = self.conversation_name(&s);
                let text: SharedString = format!(
                    "{name} · {}",
                    s.conversation.settings.backend.display_name()
                )
                .into();
                tabs = tabs.child(Self::tab_button(
                    &format!("tab-{}", sid.0),
                    text,
                    Some(status_mark(
                        SharedString::from(format!("spin-tab-{}", sid.0)),
                        s.status,
                    )),
                    selected,
                    id,
                    Tab::Conversation(*sid),
                    cx,
                ));
            }
        }
        let _ = window;
        div()
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(HEADER_HEIGHT))
                    .px_5()
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(div().flex_1().min_w_0().child(title))
                    .child(chip)
                    // The right pane, closed: this opens it again.
                    .when(!self.panes.right_open, |d| {
                        d.child(
                            div()
                                .id("right-open")
                                .p(px(4.))
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::surface_2()))
                                .child(icon(Icon::Panel, theme::fg_muted(), 16.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.panes.right_open = true;
                                    cx.notify();
                                })),
                        )
                    }),
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
    ) -> AnyElement {
        div()
            .id(SharedString::from(id.to_owned()))
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
            .into_any_element()
    }

    pub(crate) fn conversation_name(&self, s: &ConversationView) -> String {
        let name = s
            .conversation
            .settings
            .agent
            .clone()
            .unwrap_or_else(|| "Agent".to_owned());
        let flow = self.snapshot.flow(s.conversation.flow);
        let same: Vec<_> = flow.map_or_default(|c| {
            c.conversations
                .iter()
                .filter_map(|id| self.snapshot.conversations.get(id))
                .filter(|o| o.conversation.settings.agent == s.conversation.settings.agent)
                .map(|o| o.conversation.id)
                .collect()
        });
        match same.iter().position(|id| *id == s.conversation.id) {
            Some(ix) if same.len() > 1 => format!("{name} {}", ix + 1),
            _ => name,
        }
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
