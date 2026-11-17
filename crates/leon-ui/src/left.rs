//! Left pane (NAV-*): new conversation, search, pinned conversations, one
//! group per project, the archive.

use gpui::{AnyElement, Context, IntoElement, SharedString, Window, div, prelude::*, px};
use gpui_component::input::Input;
use leon_core::ConversationView;
use leon_domain::Status;

use crate::pr_view::pr_badge;
use crate::theme;
use crate::widgets::{empty, label, link, primary, status_dot};
use crate::workspace::{Screen, Workspace, value_of};

const WIDTH: f32 = 268.;

impl Workspace {
    pub(crate) fn render_left(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = value_of(&self.search, cx).to_lowercase();
        let snapshot = std::sync::Arc::clone(&self.snapshot);
        let matches = |c: &&ConversationView| {
            query.is_empty() || c.conversation.title.to_lowercase().contains(&query)
        };
        let mut body = div()
            .id("left-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_1()
            .pb_4()
            .flex()
            .flex_col()
            .child(self.render_github_rows(cx));

        let pinned: Vec<_> = snapshot
            .conversations
            .iter()
            .filter(|c| c.conversation.pinned && !c.conversation.archived)
            .filter(matches)
            .collect();
        if !pinned.is_empty() {
            body = body
                .child(label("Pinned"))
                .children(pinned.into_iter().map(|c| self.row(c, true, cx)));
        }
        for project in snapshot.projects.iter() {
            let rows: Vec<_> = snapshot
                .conversations
                .iter()
                .filter(|c| {
                    c.conversation.project == project.id
                        && !c.conversation.pinned
                        && !c.conversation.archived
                })
                .filter(matches)
                .collect();
            body = body.child(label(project.name.clone()));
            if rows.is_empty() {
                body = body.child(empty(if query.is_empty() {
                    "No conversations"
                } else {
                    "No match"
                }));
            }
            body = body.children(rows.into_iter().map(|c| self.row(c, false, cx)));
        }
        if snapshot.loaded && snapshot.projects.is_empty() {
            body = body.child(empty("Add a project folder to start."));
        }
        let archived: Vec<_> = snapshot
            .conversations
            .iter()
            .filter(|c| c.conversation.archived)
            .filter(matches)
            .collect();
        if !archived.is_empty() {
            let count = archived.len();
            let shown = self.show_archived;
            body = body.child(
                div()
                    .id("archived-toggle")
                    .cursor_pointer()
                    .child(label(format!(
                        "{} Archived ({count})",
                        if shown { "▾" } else { "▸" }
                    )))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_archived = !this.show_archived;
                        cx.notify();
                    })),
            );
            if shown {
                body = body.children(archived.into_iter().map(|c| self.row(c, true, cx)));
            }
        }

        div()
            .w(px(WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::border())
            .child(
                div()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(
                        primary(
                            "new-conversation",
                            "New conversation",
                            cx.listener(|this, _, _, cx| this.show_new_conversation(cx)),
                        )
                        .flex()
                        .justify_center(),
                    )
                    .child(Input::new(&self.search))
                    .child(link(
                        "add-project",
                        "Add project folder",
                        cx.listener(|this, _, _, cx| this.add_project(cx)),
                    )),
            )
            .child(body)
            .into_any_element()
    }

    fn row(&self, c: &ConversationView, show_project: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = c.conversation.id;
        let selected = self.screen == Screen::Conversation(id);
        let unread = c.conversation.unread;
        let meta = self.row_meta(c, show_project);
        div()
            .id(SharedString::from(format!("conv-{}", id.0)))
            .mx_1()
            .px_2()
            .py(px(5.))
            .rounded_md()
            .cursor_pointer()
            .when(selected, |s| s.bg(theme::accent_soft()))
            .when(!selected, |s| s.hover(|s| s.bg(theme::surface_2())))
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(status_dot(c.status))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .when(unread, |s| s.font_weight(gpui::FontWeight::SEMIBOLD))
                            .child(c.conversation.title.clone()),
                    )
                    .children(self.snapshot.prs.get(&id).map(pr_badge))
                    .when(unread, |s| {
                        s.child(div().size(px(7.)).rounded_full().bg(theme::accent()))
                    }),
            )
            .child(
                div()
                    .pl(px(17.))
                    .text_size(px(11.5))
                    .text_color(match c.status {
                        Status::NeedsInput => theme::warn(),
                        Status::Error => theme::err(),
                        _ => theme::fg_muted(),
                    })
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(meta),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select(id, cx)))
            .into_any_element()
    }

    fn row_meta(&self, c: &ConversationView, show_project: bool) -> String {
        let project = if show_project {
            self.snapshot
                .projects
                .iter()
                .find(|p| p.id == c.conversation.project)
                .map_or_default(|p| format!("{} · ", p.name))
        } else {
            String::new()
        };
        let agents = c.sessions.len();
        let what = match c.status {
            Status::Running => "Working".to_owned(),
            Status::NeedsInput => "Needs your answer".to_owned(),
            Status::Error => "Stopped with an error".to_owned(),
            Status::Idle => ago(c.conversation.updated_at),
        };
        if agents > 1 {
            format!("{project}{agents} agents · {what}")
        } else {
            format!("{project}{what}")
        }
    }
}

/// "now", "5m", "3h", "2d".
pub(crate) fn ago(at_ms: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let now = i64::try_from(now).unwrap_or(i64::MAX);
    let minutes = (now - at_ms).max(0) / 60_000;
    match minutes {
        0 => "now".to_owned(),
        m if m < 60 => format!("{m}m"),
        m if m < 60 * 24 => format!("{}h", m / 60),
        m => format!("{}d", m / (60 * 24)),
    }
}
