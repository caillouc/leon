//! Left pane (NAV-*): new flow, search, pinned flows, one
//! group per project, the archive.

use gpui::{AnyElement, Context, IntoElement, SharedString, Window, div, prelude::*};
use gpui_component::input::Input;
use leon_core::{FlowView, Intent};
use leon_domain::{Attention, ProjectId};

use crate::drag_line::{self, DragLine, DraggedProject, Lead};
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
        // While a project is dragged, the projects show in the order a drop
        // would give them (NAV-2); a drag let go elsewhere changes nothing.
        if !cx.has_active_drag() {
            self.project_drag = None;
        }
        let order: Vec<ProjectId> = snapshot.projects.iter().map(|p| p.id).collect();
        let shown = drag_line::shown(&order, self.project_drag);
        let before: std::rc::Rc<[ProjectId]> = order.clone().into();
        body = body.on_drop(drag_line::dropped::<DraggedProject, _>(
            order,
            |this| &mut this.project_drag,
            Intent::OrderProjects,
            cx,
        ));
        let projects = shown
            .iter()
            .filter_map(|id| snapshot.projects.iter().find(|p| p.id == *id));
        for project in projects {
            let rows: Vec<_> = snapshot
                .flows
                .iter()
                .filter(|c| in_project(c, project.id))
                .filter(matches)
                .collect();
            // A search shows matches even in a folded project, and a folded
            // project still shows the open flow. The project being dragged
            // shows its line alone, moving among the others.
            let dragged = self.project_drag.is_some_and(|(id, _)| id == project.id);
            // A folded project shows its most pressing flow's colour.
            let attention = rows
                .iter()
                .map(|c| Attention::of(c.status, c.flow.unread))
                .max()
                .and_then(crate::flow_row::attention_color);
            // A project without flows shows folded: nothing to unfold.
            let no_flows = !has_flows(&snapshot.flows, project.id);
            body = body.child(Self::project_header(
                project,
                no_flows,
                (std::rc::Rc::clone(&before), dragged),
                rows.len(),
                attention,
                self.panes.left,
                cx,
            ));
            if dragged {
                continue;
            }
            if (project.collapsed || no_flows) && query.is_empty() {
                let open = rows
                    .into_iter()
                    .filter(|c| self.screen == Screen::Flow(c.flow.id));
                body = body.children(open.map(|c| self.row(c, false, cx)));
                continue;
            }
            if rows.is_empty() {
                body = body.child(empty("No match"));
            }
            body = body.children(rows.into_iter().map(|c| self.row(c, false, cx)));
        }
        if snapshot.loaded && snapshot.projects.is_empty() {
            body = body.child(empty("Add a project folder to start."));
        }
        let archived: Vec<_> = snapshot
            .flows
            .iter()
            .filter(|c| c.flow.archived.is_some())
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
                    .py(crate::theme::rem(5.))
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_2()))
                    .text_size(crate::theme::rem(12.5))
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
            .w(crate::theme::rem(self.panes.left))
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
                    .h(crate::theme::rem(HEADER_HEIGHT))
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
                            .size(crate::theme::rem(30.))
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
            .child(crate::widgets::text_action(
                "add-project",
                "+ Add project",
                cx.listener(|this, _, _, cx| this.add_project(cx)),
            ))
            .into_any_element()
    }

    /// A project's row: click to fold or unfold its flows (NAV-2); its
    /// folder, open or closed, turning into the fold arrow on hover; name,
    /// a dot when a folded flow needs you, count, + (on hover) to start one
    /// in it (NAV-10); right-click to remove it; drag it among the others:
    /// its line, folded, follows the mouse up and down while the others
    /// make room, and stays where let go (`no_flows`: it has none, so it
    /// shows folded, with nothing to unfold; `order`: the projects' as it
    /// was before the drag; `dragged`: it is the one moving, its place left
    /// empty; `pane`: the left pane's width).
    fn project_header(
        project: &leon_domain::Project,
        no_flows: bool,
        (order, dragged): (std::rc::Rc<[ProjectId]>, bool),
        count: usize,
        attention: Option<gpui::Rgba>,
        pane: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (id, collapsed) = (project.id, project.collapsed || no_flows);
        let group = SharedString::from(format!("project-{}", id.0));
        let name = SharedString::from(project.name.clone());
        div()
            .id(group.clone())
            .group(group.clone())
            // Its place, kept empty: the line itself follows the mouse.
            .when(dragged, |d| d.opacity(0.))
            .on_drag(DraggedProject(id), move |_, grip, window, cx| {
                // The row's width: the pane less its padding, the row's
                // margins and its border.
                let width = crate::theme::rem(pane - 17.).to_pixels(window.rem_size());
                cx.new(|cx| {
                    DragLine::new(
                        (Lead::Folder, name.clone(), count),
                        width,
                        grip.x,
                        window,
                        cx,
                    )
                })
            })
            // Over this row, the dragged project takes its place.
            .on_drag_move(drag_line::over(
                id,
                order,
                |d: &DraggedProject| d.0,
                |this| &mut this.project_drag,
                cx,
            ))
            .mx_1()
            // Projects well apart, so each flow's project is plain.
            .mt_4()
            .px_2()
            .h(crate::theme::rem(30.))
            .rounded_md()
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_2()))
            .on_click(cx.listener(move |this, _, _, _| {
                if !no_flows {
                    this.core.send(Intent::CollapseProject(id, !collapsed));
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.row_menu = Some((MenuFor::Project(id), e.position));
                    cx.notify();
                }),
            )
            .child(
                div()
                    .relative()
                    .size(crate::theme::rem(15.))
                    .flex_none()
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .when(!no_flows, |d| {
                                d.group_hover(group.clone(), |s| s.opacity(0.))
                            })
                            .child(crate::icons::folder(!collapsed, theme::fg_muted(), 15.)),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(0.)
                            .when(!no_flows, |d| {
                                d.group_hover(group.clone(), |s| s.opacity(1.))
                            })
                            .child(chevron(!collapsed, theme::fg_muted(), 14.)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    // A heading over its flows, not one of them: smaller,
                    // quieter.
                    .text_size(crate::theme::rem(12.5))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::fg_muted())
                    .child(project.name.clone()),
            )
            .children(
                attention
                    .filter(|_| collapsed)
                    .map(|color| div().size(crate::theme::rem(8.)).rounded_full().bg(color)),
            )
            .child(
                div()
                    .text_size(crate::theme::rem(12.))
                    .text_color(theme::fg_faint())
                    .child(count.to_string()),
            )
            // No room until hovered: the count ends the row, then moves
            // left of the +.
            .child(
                div()
                    .id(SharedString::from(format!("new-in-room-{}", id.0)))
                    .flex_none()
                    .w_0()
                    .ml(crate::theme::rem(-8.))
                    .overflow_hidden()
                    .group_hover(group, |s| s.w(crate::theme::rem(20.)).ml_0())
                    .child(
                        div()
                            .id(SharedString::from(format!("new-in-{}", id.0)))
                            .p(crate::theme::rem(3.))
                            .rounded_md()
                            .hover(|s| s.bg(theme::surface()))
                            .child(icon(Icon::Plus, theme::fg_muted(), 14.))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.open_new_flow(Some(id), None, window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }
}

/// How long ago, as a phrase: "just now", "5m ago".
pub(crate) fn since(at_ms: i64) -> String {
    match ago(at_ms).as_str() {
        "now" => "just now".to_owned(),
        short => format!("{short} ago"),
    }
}

/// "now", "5m", "3h", "2d".
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
    c.flow.pinned && c.flow.archived.is_none()
}

/// Whether `project` has flows to show under it; without, it shows folded
/// (NAV-2).
fn has_flows(flows: &[FlowView], project: ProjectId) -> bool {
    flows.iter().any(|c| in_project(c, project))
}

/// Shown under its project.
fn in_project(c: &FlowView, project: leon_domain::ProjectId) -> bool {
    c.flow.project == project && !c.flow.pinned && c.flow.archived.is_none()
}

#[cfg(test)]
mod tests {
    use leon_domain::{FlowId, ProjectId};

    use super::has_flows;
    use crate::test_data::flow_view;

    /// A project whose flows are all pinned or archived has none under it,
    /// so it shows folded (NAV-2).
    #[test]
    fn a_project_without_flows_under_it_shows_folded() {
        let (pinned, archived) = (true, Some(1));
        let cases = [
            ("none", vec![], false),
            ("one", vec![(false, None)], true),
            ("pinned", vec![(pinned, None)], false),
            ("archived", vec![(false, archived)], false),
            (
                "one of two archived",
                vec![(false, archived), (false, None)],
                true,
            ),
        ];
        for (name, flows, expected) in cases {
            let flows: Vec<_> = flows
                .into_iter()
                .zip(1..)
                .map(|((pinned, archived), id)| {
                    let mut flow = flow_view(FlowId(id), ProjectId(1));
                    flow.flow.pinned = pinned;
                    flow.flow.archived = archived;
                    flow
                })
                .collect();
            assert_eq!(has_flows(&flows, ProjectId(1)), expected, "{name}");
            assert!(!has_flows(&flows, ProjectId(2)), "{name}: another project");
        }
    }
}
