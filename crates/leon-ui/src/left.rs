//! Left pane (NAV-*): new flow, search, pinned flows, one
//! group per project, the archive.

use gpui::{AnyElement, Context, IntoElement, SharedString, Window, div, prelude::*, px};
use gpui_component::input::Input;
use leon_core::{FlowView, Intent};
use leon_domain::Attention;

use crate::icons::{Icon, chevron, icon};
use crate::row_menu::MenuFor;
use crate::theme;
use crate::widgets::{HEADER_HEIGHT, empty, label};
use crate::workspace::{Screen, Workspace, value_of};

impl Workspace {
    pub(crate) fn render_left(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = value_of(&self.search, cx).to_lowercase();
        let snapshot = std::sync::Arc::clone(&self.snapshot);
        let matches =
            |c: &&FlowView| query.is_empty() || c.flow.title.to_lowercase().contains(&query);
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
            .flows
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
                .flows
                .iter()
                .filter(|c| in_project(c, project.id))
                .filter(matches)
                .collect();
            // A search shows matches even in a folded project, and a folded
            // project still shows the open flow.
            let folded = project.collapsed && query.is_empty();
            // A folded project shows its most pressing flow's colour.
            let attention = rows
                .iter()
                .map(|c| Attention::of(c.status, c.flow.unread))
                .max()
                .and_then(crate::flow_row::attention_color);
            body = body.child(Self::project_header(project, rows.len(), attention, cx));
            if folded {
                let open = rows
                    .into_iter()
                    .filter(|c| self.screen == Screen::Flow(c.flow.id));
                body = body.children(open.map(|c| self.row(c, false, cx)));
                continue;
            }
            if rows.is_empty() {
                body = body.child(empty(if query.is_empty() {
                    "No flows"
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
            .flows
            .iter()
            .filter(|c| c.flow.archived)
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
                            .child(icon(Icon::NewFlow, theme::on_accent(), 16.))
                            .on_click(cx.listener(|this, _, _, cx| this.show_new_flow(cx))),
                    ),
            )
            .child(body)
            .children(self.render_row_menu(cx))
            .child(self.render_bottom_nav(cx))
            .into_any_element()
    }

    /// Flows in the left pane's order: pinned, then each project's.
    pub(crate) fn left_order(&self) -> Vec<leon_domain::FlowId> {
        let all = &self.snapshot.flows;
        let pinned = all.iter().filter(|c| is_pinned(c));
        let projects = self
            .snapshot
            .projects
            .iter()
            .flat_map(|p| all.iter().filter(move |c| in_project(c, p.id)));
        pinned.chain(projects).map(|c| c.flow.id).collect()
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

    /// A project's row: click to fold or unfold its flows (NAV-2);
    /// chevron, name, count, a dot when a folded flow needs you, +
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
                        this.open_new_flow(Some(id), None, window, cx);
                    })),
            )
            .into_any_element()
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
fn is_pinned(c: &FlowView) -> bool {
    c.flow.pinned && !c.flow.archived
}

/// Shown under its project.
fn in_project(c: &FlowView, project: leon_domain::ProjectId) -> bool {
    c.flow.project == project && !c.flow.pinned && !c.flow.archived
}
