//! Main pane for a conversation: header with actions, agent tabs, transcript
//! and composer.

use gpui::{
    AnyElement, ClipboardItem, Context, IntoElement, SharedString, Window, div, prelude::*, px,
};
use gpui_component::input::{Input, InputState};
use leon_core::{Intent, SessionView};
use leon_domain::{ConversationId, Status};

use crate::theme;
use crate::widgets::{button, empty, status_dot};
use crate::workspace::{Tab, Workspace};

impl Workspace {
    pub(crate) fn render_conversation(
        &mut self,
        id: ConversationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(view) = self.snapshot.conversation(id).cloned() else {
            return empty("This conversation no longer exists.");
        };
        let session = self.current_session();
        let body = match (self.tabs.get(&id), session) {
            (Some(Tab::Flow), _) => self.render_flow(id, window, cx),
            (_, Some(session)) => {
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
            (_, None) => empty("No agent session yet."),
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
        let title: AnyElement = match &self.rename {
            Some((rid, input)) if *rid == id => div()
                .w(px(320.))
                .child(Input::new(input))
                .into_any_element(),
            _ => div()
                .text_size(px(15.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(c.title.clone())
                .into_any_element(),
        };
        let folder = view
            .sessions
            .first()
            .and_then(|s| self.snapshot.sessions.get(s))
            .map(|s| s.session.working_dir.clone());
        let branch = self.snapshot.git.get(&id).and_then(|g| g.branch.clone());
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

        let renaming = self.rename.as_ref().is_some_and(|(rid, _)| *rid == id);
        let confirming = self.confirm_delete == Some(id);
        let (pinned, archived) = (c.pinned, c.archived);
        let menu_choices = vec![
            ("rename".to_owned(), "Rename".to_owned()),
            (
                "pin".to_owned(),
                if pinned { "Unpin" } else { "Pin to top" }.to_owned(),
            ),
            ("unread".to_owned(), "Mark as unread".to_owned()),
            (
                "archive".to_owned(),
                if archived { "Unarchive" } else { "Archive" }.to_owned(),
            ),
            ("delete".to_owned(), "Delete…".to_owned()),
        ];
        let menu = self.picker(
            "conversation-menu",
            "Actions",
            menu_choices,
            cx,
            move |this, value, window, cx| match value.as_str() {
                "rename" => this.start_rename(id, window, cx),
                "pin" => this.core.send(Intent::Pin(id, !pinned)),
                "unread" => this.core.send(Intent::MarkUnread(id)),
                "archive" => this.core.send(Intent::Archive(id, !archived)),
                "delete" => this.confirm_delete = Some(id),
                _ => {}
            },
        );
        let actions = div().flex().items_center().gap_1().when(renaming, |d| {
            d.child(button(
                "rename-save",
                "Save",
                cx.listener(move |this, _, window, cx| this.finish_rename(window, cx)),
            ))
        });
        let actions = actions.child(menu);
        let confirm_bar =
            confirming.then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .child(div().text_color(theme::err()).child(
                        "Delete this conversation and its transcripts? This can't be undone.",
                    ))
                    .child(
                        button(
                            "delete-yes",
                            "Delete",
                            cx.listener(move |this, _, _, cx| {
                                this.core.send(Intent::Delete(id));
                                this.confirm_delete = None;
                                cx.notify();
                            }),
                        )
                        .text_color(theme::err())
                        .border_color(theme::err()),
                    )
                    .child(button(
                        "delete-no",
                        "Cancel",
                        cx.listener(|this, _, _, cx| {
                            this.confirm_delete = None;
                            cx.notify();
                        }),
                    ))
            });

        let tab = self.tabs.get(&id).copied();
        let current = self.current_session();
        let mut tabs = div().flex().gap_1().child(Self::tab_button(
            "tab-flow",
            "Flow".into(),
            None,
            tab == Some(Tab::Flow),
            id,
            Tab::Flow,
            cx,
        ));
        for sid in &view.sessions {
            if let Some(s) = self.snapshot.sessions.get(sid).cloned() {
                let selected = tab != Some(Tab::Flow) && current == Some(*sid);
                let name = self.session_name(&s);
                let text: SharedString =
                    format!("{name} · {}", s.session.settings.backend.display_name()).into();
                tabs = tabs.child(Self::tab_button(
                    &format!("tab-{}", sid.0),
                    text,
                    Some(s.status),
                    selected,
                    id,
                    Tab::Session(*sid),
                    cx,
                ));
            }
        }
        let _ = window;
        div()
            .flex_none()
            .px_4()
            .pt_3()
            .border_b_1()
            .border_color(theme::border())
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().flex_1().min_w_0().child(title))
                    .child(chip)
                    .child(actions),
            )
            .children(confirm_bar)
            .child(tabs)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn tab_button(
        id: &str,
        text: SharedString,
        status: Option<Status>,
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
            .when_some(status, |d, st| d.child(status_dot(st)))
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
        self.rename = Some((id, input));
        cx.notify();
    }

    fn finish_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some((id, input)) = self.rename.take() {
            let title = crate::workspace::value_of(&input, cx);
            if !title.trim().is_empty() {
                self.core.send(Intent::Rename(id, title));
            }
        }
        cx.notify();
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
