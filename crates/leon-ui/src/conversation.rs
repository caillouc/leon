//! Main pane for a conversation: header (title, branch), tabs (the flow and
//! each agent session), transcript and composer.

use gpui::{
    AnyElement, ClipboardItem, Context, IntoElement, SharedString, Window, div, prelude::*, px,
};
use leon_core::SessionView;
use leon_domain::{ConversationId, Status};

use crate::icons::{Icon, icon};
use crate::spinner::status_mark;
use crate::theme;
use crate::widgets::{HEADER_HEIGHT, empty};
use crate::workspace::{Tab, Workspace};

impl Workspace {
    pub(crate) fn render_conversation(
        &mut self,
        id: ConversationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(view) = self.snapshot.conversation(id).cloned() else {
            return empty("This flow no longer exists.");
        };
        let session = self.current_session();
        let body = match session {
            None if self.tabs.get(&id) == Some(&Tab::New) => {
                self.render_new_conversation(Some(id), window, cx)
            }
            None if view.sessions.is_empty() => empty("No conversation yet."),
            None => self.render_flow(id, window, cx),
            Some(session) => {
                let transcript = self.render_transcript(session, cx);
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(transcript)
                    .child(self.render_composer(session, window, cx))
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
        view: &leon_core::ConversationView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = view.conversation.id;
        let c = &view.conversation;
        let title = div()
            .text_size(px(15.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .child(c.title.clone());
        let folder = view
            .sessions
            .first()
            .and_then(|s| self.snapshot.sessions.get(s))
            .map(|s| s.session.working_dir.clone());
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

        let current = self.current_session();
        let adding = self.tabs.get(&id) == Some(&Tab::New);
        let mut tabs = div().flex().gap_1().child(Self::tab_button(
            "tab-flow",
            "Flow".into(),
            None,
            current.is_none() && !adding,
            id,
            Tab::Flow,
            cx,
        ));
        for sid in &view.sessions {
            if let Some(s) = self.snapshot.sessions.get(sid).cloned() {
                let selected = current == Some(*sid);
                let name = self.session_name(&s);
                let text: SharedString =
                    format!("{name} · {}", s.session.settings.backend.display_name()).into();
                tabs = tabs.child(Self::tab_button(
                    &format!("tab-{}", sid.0),
                    text,
                    Some(status_mark(
                        SharedString::from(format!("spin-tab-{}", sid.0)),
                        s.status,
                    )),
                    selected,
                    id,
                    Tab::Session(*sid),
                    cx,
                ));
            }
        }
        // Another conversation in this flow (FLOW-7).
        tabs = tabs.child(
            div()
                .id("tab-new")
                .px_2()
                .pb_2()
                .pt_1()
                .flex()
                .items_center()
                .gap_1()
                .border_b_2()
                .border_color(if adding {
                    theme::accent()
                } else {
                    gpui::transparent_black().into()
                })
                .text_size(px(13.))
                .text_color(if adding {
                    theme::fg()
                } else {
                    theme::fg_muted()
                })
                .cursor_pointer()
                .hover(|s| s.text_color(theme::fg()))
                .child(icon(Icon::Plus, theme::fg_muted(), 13.))
                // Short: the tab row has the flow's other tabs to hold.
                .child("New")
                .on_click(cx.listener(move |this, _, _, cx| this.open_new_in_flow(id, cx))),
        );
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
        conversation: ConversationId,
        tab: Tab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(id.to_owned()))
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
                this.tabs.insert(conversation, tab);
                cx.notify();
            }))
            .into_any_element()
    }

    pub(crate) fn session_name(&self, s: &SessionView) -> String {
        let name = s
            .session
            .settings
            .agent
            .clone()
            .unwrap_or_else(|| "Agent".to_owned());
        let conversation = self.snapshot.conversation(s.session.conversation);
        let same: Vec<_> = conversation.map_or_default(|c| {
            c.sessions
                .iter()
                .filter_map(|id| self.snapshot.sessions.get(id))
                .filter(|o| o.session.settings.agent == s.session.settings.agent)
                .map(|o| o.session.id)
                .collect()
        });
        match same.iter().position(|id| *id == s.session.id) {
            Some(ix) if same.len() > 1 => format!("{name} {}", ix + 1),
            _ => name,
        }
    }

    pub(crate) fn status_line(s: &SessionView) -> Option<String> {
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
