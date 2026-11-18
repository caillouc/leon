//! The flow tab (FLOW-4, FLOW-6): a canvas with one box per agent session
//! plus "You", arrows for prompts, reports and questions. Drag to pan, scroll
//! or pinch to zoom, drag a box to move it; click a box to open its session,
//! click an arrow's label to read its messages.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, Context, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, PinchEvent, Pixels, Point, Rgba, ScrollEvent, SharedString, Window, canvas,
    div, prelude::*, px,
};
use gpui_component::text::TextView;
use leon_core::SessionView;
use leon_domain::{AgentSessionId, ConversationId};

use crate::flow_model::{
    self, Edge, EdgeKind, Node, Pt, Rect, View, label_at, layout, rect, route, status_text,
};
use crate::flow_paint::{Arrow, legend};
use crate::theme;
use crate::widgets::{button, status_dot};
use crate::workspace::{Tab, Workspace};

/// What the flow tab remembers per conversation.
#[derive(Default)]
pub(crate) struct FlowState {
    /// Pan and zoom, and whether the user changed it (else it fits).
    views: HashMap<ConversationId, (View, bool)>,
    /// Boxes the user moved, in diagram units.
    moved: HashMap<(ConversationId, Node), Pt>,
    /// The arrow whose messages are shown.
    selected: Option<(ConversationId, Node, Node, EdgeKind)>,
    drag: Option<Drag>,
    /// Where the canvas was last painted, in window coordinates.
    area: Rc<Cell<Option<Bounds<Pixels>>>>,
}

#[derive(Debug, Clone, Copy)]
enum Drag {
    Pan {
        from: Pt,
        view: View,
    },
    Box {
        node: Node,
        from: Pt,
        start: Pt,
        moved: bool,
    },
}

fn pt(p: Point<Pixels>) -> Pt {
    Pt {
        x: f32::from(p.x),
        y: f32::from(p.y),
    }
}

pub(crate) fn color(kind: EdgeKind) -> Rgba {
    match kind {
        EdgeKind::Prompt => theme::accent(),
        EdgeKind::Report => theme::fg_muted(),
        EdgeKind::Question => theme::warn(),
    }
}

impl Workspace {
    pub(crate) fn render_flow(
        &mut self,
        id: ConversationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(conversation) = self.snapshot.conversation(id) else {
            return div().into_any_element();
        };
        let sessions: Vec<&SessionView> = conversation
            .sessions
            .iter()
            .filter_map(|s| self.snapshot.sessions.get(s))
            .collect();
        let model = flow_model::build(&sessions, &self.snapshot.transcripts);
        let mut pos = layout(&model);
        for (node, at) in &mut pos {
            if let Some(moved) = self.flow.moved.get(&(id, *node)) {
                *at = *moved;
            }
        }
        let rects: HashMap<Node, Rect> = pos.iter().map(|(n, p)| (*n, rect(*n, *p))).collect();
        let area = self.flow.area.get();
        let view = if let Some((view, true)) = self.flow.views.get(&id) {
            *view
        } else {
            // Before the first paint, the main pane's size from the window's.
            let screen = window.viewport_size();
            let (w, h) = area.map_or(
                (
                    f32::from(screen.width) - self.panes.left - self.panes.right,
                    f32::from(screen.height) - 100.,
                ),
                |b| (f32::from(b.size.width), f32::from(b.size.height)),
            );
            let fitted = flow_model::bounds(rects.values().copied()).map_or(
                View {
                    scale: 1.,
                    tx: 0.,
                    ty: 0.,
                },
                |world| View::fit(world, w, h),
            );
            self.flow.views.insert(id, (fitted, false));
            fitted
        };
        let routes: Vec<(Edge, Vec<Pt>)> = model
            .edges
            .iter()
            .filter_map(|e| {
                let (a, b) = (rects.get(&e.from)?, rects.get(&e.to)?);
                // Each kind on its own lane, so a question and an answer
                // between the same boxes never overlap.
                let off = match e.kind {
                    EdgeKind::Prompt => -18.,
                    EdgeKind::Report => 18.,
                    EdgeKind::Question => 46.,
                };
                let world = route(*a, *b, off);
                let points = world.into_iter().map(|p| view.to_screen(p)).collect();
                Some((e.clone(), points))
            })
            .collect();
        let selected = self.flow.selected.filter(|s| s.0 == id);
        let edges_canvas = {
            let routes = routes.clone();
            let store = Rc::clone(&self.flow.area);
            canvas(
                move |bounds, _, _| bounds,
                move |bounds, _, window, _| {
                    if store.get() != Some(bounds) {
                        store.set(Some(bounds));
                        window.refresh();
                    }
                    for (edge, points) in &routes {
                        let wide = selected == Some((id, edge.from, edge.to, edge.kind));
                        let arrow = Arrow {
                            kind: edge.kind,
                            wide,
                            scale: view.scale,
                        };
                        arrow.paint(window, bounds.origin, points);
                    }
                },
            )
            .absolute()
            .size_full()
        };
        let labels = routes
            .iter()
            .enumerate()
            .map(|(ix, (edge, points))| {
                let at = label_at(points);
                let key = (id, edge.from, edge.to, edge.kind);
                let on = selected == Some(key);
                let c = color(edge.kind);
                // Labels scale with the boxes, so they never dwarf them.
                let s = view.scale.max(0.7);
                div()
                    .id(("flow-label", ix))
                    .absolute()
                    .left(px(at.x - 50.))
                    .top(px(at.y - 10. * s))
                    .w(px(100.))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px(px(7. * s))
                            .py(px(1.5 * s))
                            .rounded_full()
                            .border_1()
                            .border_color(c)
                            .bg(if on { c } else { theme::surface() })
                            .text_color(if on { theme::window() } else { c })
                            .text_size(px(11. * s))
                            .whitespace_nowrap()
                            .child(edge.label()),
                    )
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.flow.selected = if this.flow.selected == Some(key) {
                            None
                        } else {
                            Some(key)
                        };
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        let boxes: Vec<AnyElement> = model
            .nodes
            .iter()
            .filter_map(|node| {
                let r = rects.get(node)?;
                Some(self.flow_box(id, *node, *r, view, &sessions, cx))
            })
            .collect();
        let inspector = selected.and_then(|key| {
            let edge = model
                .edges
                .iter()
                .find(|e| (id, e.from, e.to, e.kind) == key)?;
            Some(self.flow_inspector(id, edge, cx))
        });
        div()
            .id("flow")
            .relative()
            .flex_1()
            .size_full()
            .overflow_hidden()
            .bg(theme::window())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, _| {
                    let view = this.flow.views.get(&id).map_or(view, |v| v.0);
                    this.flow.drag = Some(Drag::Pan {
                        from: pt(e.position),
                        view,
                    });
                }),
            )
            .on_mouse_move(cx.listener(move |this, e: &MouseMoveEvent, _, cx| {
                this.flow_drag(id, e, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, e: &gpui::MouseUpEvent, _, cx| {
                    this.flow_release(id, pt(e.position), cx);
                }),
            )
            .on_scroll(cx.listener(move |this, e: &ScrollEvent, _, cx| {
                let dy = f32::from(e.delta.pixel_delta(px(16.)).y);
                this.flow_zoom(id, pt(e.position), (dy * 0.0025).exp(), cx);
            }))
            .on_pinch(cx.listener(move |this, e: &PinchEvent, _, cx| {
                this.flow_zoom(id, pt(e.position), 1. + e.delta, cx);
            }))
            .child(edges_canvas)
            .children(boxes)
            .children(labels)
            .child(self.flow_controls(id, view, cx))
            .child(legend())
            .children(inspector)
            .into_any_element()
    }

    fn flow_box(
        &self,
        id: ConversationId,
        node: Node,
        r: Rect,
        view: View,
        sessions: &[&SessionView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let at = view.to_screen(Pt { x: r.x, y: r.y });
        let s = view.scale;
        let shell = div()
            .id(SharedString::from(format!("flow-node-{node:?}")))
            .absolute()
            .left(px(at.x))
            .top(px(at.y))
            .w(px(r.w * s))
            .h(px(r.h * s))
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let start = this
                        .flow
                        .moved
                        .get(&(id, node))
                        .copied()
                        .unwrap_or(Pt { x: r.x, y: r.y });
                    this.flow.drag = Some(Drag::Box {
                        node,
                        from: pt(e.position),
                        start,
                        moved: false,
                    });
                }),
            );
        let Node::Session(sid) = node else {
            let first = sessions
                .first()
                .and_then(|f| self.snapshot.transcripts.get(&f.session.id))
                .and_then(|t| {
                    t.entries.iter().find_map(|e| match &e.entry {
                        leon_domain::Entry::User { text } => {
                            Some(text.lines().next().unwrap_or_default().to_owned())
                        }
                        _ => None,
                    })
                })
                .unwrap_or_default();
            return shell
                .rounded_full()
                .border_2()
                .border_color(theme::ok())
                .bg(theme::add_bg())
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .px(px(16. * s))
                .child(
                    div()
                        .text_size(px(13. * s))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("You"),
                )
                .child(
                    div()
                        .max_w_full()
                        .text_size(px(11. * s))
                        .text_color(theme::fg_muted())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(first),
                )
                .into_any_element();
        };
        let Some(session) = sessions.iter().find(|x| x.session.id == sid) else {
            return shell.into_any_element();
        };
        let settings = &session.session.settings;
        let waiting_for = sessions
            .iter()
            .find(|x| x.session.started_by == Some(sid) && x.status == leon_domain::Status::Running)
            .map(|x| self.session_name(x));
        let border = match session.status {
            leon_domain::Status::Running => theme::accent(),
            leon_domain::Status::NeedsInput => theme::warn(),
            leon_domain::Status::Error => theme::err(),
            leon_domain::Status::Idle => theme::border_strong(),
        };
        let (messages, tools) = self.snapshot.transcripts.get(&sid).map_or((0, 0), |t| {
            t.entries.iter().fold((0, 0), |(m, k), e| match e.entry {
                leon_domain::Entry::Tool { .. } => (m, k + 1),
                leon_domain::Entry::User { .. }
                | leon_domain::Entry::Agent { .. }
                | leon_domain::Entry::FromAgent { .. } => (m + 1, k),
                _ => (m, k),
            })
        });
        let (model, effort) = self.model_and_effort(settings);
        let model = format!("{model} · effort {effort}");
        let agent = agent_color(sessions, session, |x| self.session_name(x));
        shell
            .rounded(px(10. * s))
            .line_height(gpui::relative(1.25))
            .border_2()
            .border_color(border)
            .bg(theme::surface())
            .p(px(10. * s))
            .flex()
            .flex_col()
            .gap(px(3. * s))
            .cursor_pointer()
            // The agent's colour, as a band along the top, its corners
            // following the box's.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(4. * s))
                    // The box's inner curve: its radius less its border.
                    .rounded_t(px((10. * s - 2.).max(0.)))
                    .bg(agent),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6. * s))
                    .text_size(px(13. * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(status_dot(session.status))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(agent)
                            .child(self.session_name(session)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .px(px(5. * s))
                            .rounded(px(4.))
                            .bg(theme::surface_2())
                            .text_size(px(10.5 * s))
                            .text_color(theme::fg_muted())
                            .child(settings.backend.display_name()),
                    ),
            )
            .child(
                div()
                    .font_family(theme::MONO)
                    .text_size(px(11. * s))
                    .text_color(theme::fg_faint())
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(model),
            )
            .child(
                div()
                    .text_size(px(12. * s))
                    .text_color(match session.status {
                        leon_domain::Status::Running => theme::accent(),
                        leon_domain::Status::NeedsInput => theme::warn(),
                        leon_domain::Status::Error => theme::err(),
                        leon_domain::Status::Idle => theme::fg_muted(),
                    })
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(status_text(session, waiting_for.as_deref())),
            )
            .child(
                div()
                    .mt_auto()
                    .text_size(px(11. * s))
                    .text_color(theme::fg_faint())
                    .child({
                        // The plan's progress, when the agent keeps one.
                        let done = session
                            .plan
                            .iter()
                            .filter(|p| p.status == leon_domain::StepStatus::Done)
                            .count();
                        let plan = if session.plan.is_empty() {
                            String::new()
                        } else {
                            format!(" · plan {done}/{}", session.plan.len())
                        };
                        format!("{messages} messages · {tools} tool calls{plan}")
                    }),
            )
            .into_any_element()
    }

    fn flow_drag(&mut self, id: ConversationId, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        if e.pressed_button != Some(MouseButton::Left) {
            self.flow.drag = None;
            return;
        }
        let now = pt(e.position);
        match self.flow.drag {
            Some(Drag::Pan { from, view }) => {
                let moved = View {
                    tx: view.tx + now.x - from.x,
                    ty: view.ty + now.y - from.y,
                    ..view
                };
                self.flow.views.insert(id, (moved, true));
            }
            Some(Drag::Box {
                node,
                from,
                start,
                moved,
            }) => {
                let scale = self.flow.views.get(&id).map_or(1., |v| v.0.scale);
                let (dx, dy) = (now.x - from.x, now.y - from.y);
                let moved = moved || dx.abs() + dy.abs() > 3.;
                if moved {
                    let at = Pt {
                        x: start.x + dx / scale,
                        y: start.y + dy / scale,
                    };
                    self.flow.moved.insert((id, node), at);
                }
                self.flow.drag = Some(Drag::Box {
                    node,
                    from,
                    start,
                    moved,
                });
            }
            None => return,
        }
        cx.notify();
    }

    /// End of a drag. A box that did not move was clicked: open its session.
    /// A click on the background (no pan) closes the message panel.
    fn flow_release(&mut self, id: ConversationId, at: Pt, cx: &mut Context<Self>) {
        if let Some(Drag::Pan { from, .. }) = self.flow.drag
            && (at.x - from.x).abs() + (at.y - from.y).abs() <= 3.
        {
            self.flow.selected = None;
        }
        if let Some(Drag::Box {
            node: Node::Session(session),
            moved: false,
            ..
        }) = self.flow.drag
        {
            self.tabs.insert(id, Tab::Session(session));
        }
        self.flow.drag = None;
        cx.notify();
    }

    fn flow_zoom(&mut self, id: ConversationId, at: Pt, factor: f32, cx: &mut Context<Self>) {
        let Some(area) = self.flow.area.get() else {
            return;
        };
        let Some((view, _)) = self.flow.views.get(&id).copied() else {
            return;
        };
        let origin = pt(area.origin);
        let zoomed = view.zoom_at(at.x - origin.x, at.y - origin.y, factor);
        self.flow.views.insert(id, (zoomed, true));
        cx.notify();
    }

    fn flow_controls(&self, id: ConversationId, view: View, cx: &mut Context<Self>) -> AnyElement {
        let center = self
            .flow
            .area
            .get()
            .map_or(Pt { x: 450., y: 320. }, |b| Pt {
                x: f32::from(b.origin.x + b.size.width / 2.),
                y: f32::from(b.origin.y + b.size.height / 2.),
            });
        let moved = self.flow.moved.keys().any(|(c, _)| *c == id);
        div()
            .absolute()
            .right_3()
            .bottom_3()
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::surface())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(button(
                "flow-zoom-out",
                "−",
                cx.listener(move |this, _, _, cx| this.flow_zoom(id, center, 1. / 1.2, cx)),
            ))
            .child(
                div()
                    .w(px(44.))
                    .flex()
                    .justify_center()
                    .font_family(theme::MONO)
                    .text_size(px(11.))
                    .text_color(theme::fg_muted())
                    .child(format!("{:.0}%", view.scale * 100.)),
            )
            .child(button(
                "flow-zoom-in",
                "+",
                cx.listener(move |this, _, _, cx| this.flow_zoom(id, center, 1.2, cx)),
            ))
            .child(button(
                "flow-reset-view",
                "Fit",
                cx.listener(move |this, _, _, cx| {
                    this.flow.views.remove(&id);
                    cx.notify();
                }),
            ))
            .child(
                button(
                    "flow-reset-layout",
                    "Reset layout",
                    cx.listener(move |this, _, _, cx| {
                        this.flow.moved.retain(|(c, _), _| *c != id);
                        cx.notify();
                    }),
                )
                .when(!moved, |b| b.opacity(0.5)),
            )
            .into_any_element()
    }

    fn flow_inspector(
        &self,
        id: ConversationId,
        edge: &Edge,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = |node: Node| match node {
            Node::You => "You".to_owned(),
            Node::Session(s) => self
                .snapshot
                .sessions
                .get(&s)
                .map_or_default(|v| self.session_name(v)),
        };
        let agent: Option<AgentSessionId> = match (edge.kind, edge.from, edge.to) {
            (EdgeKind::Prompt, _, Node::Session(s)) | (_, Node::Session(s), _) => Some(s),
            _ => None,
        };
        let items = edge.items.iter().enumerate().map(|(ix, item)| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .pt_2()
                .border_t_1()
                .border_color(theme::border())
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::fg_faint())
                        .child(format!(
                            "{} · {} ago",
                            item.label,
                            crate::left::ago(item.at)
                        )),
                )
                .child(TextView::markdown(
                    SharedString::from(format!("flow-item-{ix}")),
                    SharedString::from(item.text.clone()),
                ))
        });
        div()
            .id("flow-inspector")
            .absolute()
            .top_3()
            .right_3()
            .w(px(340.))
            .max_h(px(520.))
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::surface())
            .text_size(px(12.5))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(format!(
                        "{} → {}",
                        name(edge.from),
                        name(edge.to)
                    )))
                    .child(div().text_color(theme::fg_muted()).child(edge.label()))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("flow-inspector-close")
                            .cursor_pointer()
                            .text_color(theme::fg_faint())
                            .child("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.flow.selected = None;
                                cx.notify();
                            })),
                    ),
            )
            .children(items)
            .children(agent.map(|s| {
                div().pt_2().child(button(
                    "flow-open-session",
                    format!("Open {}", name(Node::Session(s))),
                    cx.listener(move |this, _, _, cx| {
                        this.tabs.insert(id, Tab::Session(s));
                        cx.notify();
                    }),
                ))
            }))
            .into_any_element()
    }
}

/// An agent's colour in the diagram: one per agent name, in the order the
/// agents first appear in the conversation, so two agents never share one.
fn agent_color(
    sessions: &[&SessionView],
    session: &SessionView,
    name: impl Fn(&SessionView) -> String,
) -> Rgba {
    let mut names: Vec<String> = Vec::new();
    for s in sessions {
        let n = name(s);
        if !names.contains(&n) {
            names.push(n);
        }
    }
    let ix = names.iter().position(|n| *n == name(session)).unwrap_or(0);
    theme::agent_color(ix)
}
