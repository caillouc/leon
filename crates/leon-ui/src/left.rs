//! Left pane (NAV-*): new conversation, search, pinned conversations, one
//! group per project, the archive.

use gpui::{AnyElement, Context, IntoElement, SharedString, Window, div, prelude::*, px};
use gpui_component::input::Input;
use leon_core::{ConversationView, Intent};
use leon_domain::Status;

use crate::icons::{Icon, chevron, icon};
use crate::pr_view::pr_badge;
use crate::row_menu::MenuFor;
use crate::theme;
use crate::widgets::{HEADER_HEIGHT, empty, label, status_dot};
use crate::workspace::{Screen, Workspace, value_of};

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
            .filter(|c| is_pinned(c))
            .filter(matches)
            .collect();
        if !pinned.is_empty() {
            body = body
                .child(label("Pinned"))
                .children(pinned.into_iter().map(|c| self.row(c, true, cx)));
        }
        body = body.child(Self::projects_header(cx));
        for project in snapshot.projects.iter() {
            let rows: Vec<_> = snapshot
                .conversations
                .iter()
                .filter(|c| in_project(c, project.id))
                .filter(matches)
                .collect();
            // A search shows matches even in a folded project, and a folded
            // project still shows the open conversation.
            let folded = project.collapsed && query.is_empty();
            let attention = rows
                .iter()
                .map(|c| c.status)
                .max_by_key(|s| match s {
                    Status::Error => 2,
                    Status::NeedsInput => 1,
                    Status::Running | Status::Idle => 0,
                })
                .and_then(|s| match s {
                    Status::Error => Some(theme::err()),
                    Status::NeedsInput => Some(theme::warn()),
                    Status::Running | Status::Idle => None,
                });
            body = body.child(Self::project_header(project, rows.len(), attention, cx));
            if folded {
                let open = rows
                    .into_iter()
                    .filter(|c| self.screen == Screen::Conversation(c.conversation.id));
                body = body.children(open.map(|c| self.row(c, false, cx)));
                continue;
            }
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
                    .mx_1()
                    .mt_3()
                    .px_2()
                    .py(px(5.))
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_2()))
                    .text_size(px(12.5))
                    .text_color(theme::fg_muted())
                    .child(chevron(shown, theme::fg_muted(), 14.))
                    .child("Archived")
                    .child(div().text_color(theme::fg_faint()).child(count.to_string()))
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
            .w(px(self.panes.left))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::border())
            .child(
                div()
                    .flex_none()
                    .h(px(HEADER_HEIGHT))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(div().flex_1().min_w_0().child(Input::new(&self.search)))
                    .child(
                        div()
                            .id("new-conversation")
                            .flex_none()
                            .size(px(30.))
                            .rounded_md()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(theme::accent())
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.9))
                            .child(icon(Icon::NewConversation, theme::on_accent(), 16.))
                            .on_click(cx.listener(|this, _, _, cx| this.show_new_conversation(cx))),
                    ),
            )
            .child(body)
            .children(self.render_row_menu(cx))
            .child(self.render_bottom_nav(cx))
            .into_any_element()
    }

    /// Conversations in the left pane's order: pinned, then each project's.
    pub(crate) fn left_order(&self) -> Vec<leon_domain::ConversationId> {
        let all = &self.snapshot.conversations;
        let pinned = all.iter().filter(|c| is_pinned(c));
        let projects = self
            .snapshot
            .projects
            .iter()
            .flat_map(|p| all.iter().filter(move |c| in_project(c, p.id)));
        pinned.chain(projects).map(|c| c.conversation.id).collect()
    }

    /// "Projects" and its own button to add one (NAV-1).
    fn projects_header(cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_end()
            .justify_between()
            .pr_2()
            .child(label("Projects"))
            .child(
                div()
                    .id("add-project")
                    .px(px(6.))
                    .py(px(1.))
                    .rounded_md()
                    .text_size(px(11.5))
                    .text_color(theme::fg_muted())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_2()).text_color(theme::fg()))
                    .child("+ Add project")
                    .on_click(cx.listener(|this, _, _, cx| this.add_project(cx))),
            )
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
                    .child(match &self.rename {
                        Some((rid, input, _)) if *rid == id => div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(input))
                            .into_any_element(),
                        _ => div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .when(unread, |s| s.font_weight(gpui::FontWeight::SEMIBOLD))
                            .child(c.conversation.title.clone())
                            .into_any_element(),
                    })
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
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.row_menu = Some((MenuFor::Conversation(id), e.position));
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    /// A project's row: click to fold or unfold its conversations (NAV-2);
    /// chevron, name, count, a dot when a folded conversation needs you, +
    /// (on hover) to start one in it (NAV-10); right-click to remove it.
    fn project_header(
        project: &leon_domain::Project,
        count: usize,
        attention: Option<gpui::Rgba>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (id, collapsed) = (project.id, project.collapsed);
        let group = SharedString::from(format!("project-{}", id.0));
        div()
            .id(group.clone())
            .group(group.clone())
            .mx_1()
            .mt_2()
            .px_2()
            .h(px(30.))
            .rounded_md()
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_2()))
            .on_click(cx.listener(move |this, _, _, _| {
                this.core.send(Intent::CollapseProject(id, !collapsed));
            }))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.row_menu = Some((MenuFor::Project(id), e.position));
                    cx.notify();
                }),
            )
            .child(chevron(!collapsed, theme::fg_muted(), 14.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(project.name.clone()),
            )
            .children(
                attention
                    .filter(|_| collapsed)
                    .map(|color| div().size(px(8.)).rounded_full().bg(color)),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::fg_faint())
                    .child(count.to_string()),
            )
            .child(
                div()
                    .id(SharedString::from(format!("new-in-{}", id.0)))
                    .p(px(3.))
                    .rounded_md()
                    .opacity(0.)
                    .group_hover(group, |s| s.opacity(1.))
                    .hover(|s| s.bg(theme::surface()))
                    .child(icon(Icon::Plus, theme::fg_muted(), 14.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_new_conversation(Some(id), None, window, cx);
                    })),
            )
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
/// How long ago, as a phrase: "just now", "5m ago".
pub(crate) fn since(at_ms: i64) -> String {
    match ago(at_ms).as_str() {
        "now" => "just now".to_owned(),
        short => format!("{short} ago"),
    }
}

pub(crate) fn ago(at_ms: i64) -> String {
    let minutes = (now_ms() - at_ms).max(0) / 60_000;
    match minutes {
        0 => "now".to_owned(),
        m if m < 60 => format!("{m}m"),
        m if m < 60 * 24 => format!("{}h", m / 60),
        m => format!("{}d", m / (60 * 24)),
    }
}

/// Now, in Unix milliseconds.
pub(crate) fn now_ms() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    i64::try_from(now).unwrap_or(i64::MAX)
}

/// Shown under Pinned.
fn is_pinned(c: &ConversationView) -> bool {
    c.conversation.pinned && !c.conversation.archived
}

/// Shown under its project.
fn in_project(c: &ConversationView, project: leon_domain::ProjectId) -> bool {
    c.conversation.project == project && !c.conversation.pinned && !c.conversation.archived
}
