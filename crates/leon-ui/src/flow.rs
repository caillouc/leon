//! The flow tab (FLOW-4, FLOW-6): a canvas with one box per conversation
//! plus "You", arrows for prompts, reports and questions. Drag to pan, scroll
//! or pinch to zoom, drag a box to move it; click a box to open its conversation,
//! click an arrow's label to read its messages.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, Context, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    MouseMoveEvent, PinchEvent, Pixels, Point, Rgba, ScrollEvent, SharedString, Window, canvas,
    div, prelude::*, px, relative,
};
use leon_core::ConversationView;
use leon_domain::{ConversationId, FlowId};

use crate::flow_model::{self, Edge, EdgeKind, Node, Pt, Rect, View, rect, status_text};
use crate::flow_paint::{Arrow, legend};
use crate::row_menu::MenuFor;
use crate::spinner::status_mark;
use crate::theme;
use crate::widgets::{button, primary};
use crate::workspace::{Tab, Workspace};

/// What the flow tab remembers per flow.
/// A line of arrows, by its flow and its first arrow: from, to, kind.
type LineKey = (FlowId, Node, Node, EdgeKind);

#[derive(Default)]
pub(crate) struct FlowState {
    /// Pan and zoom, and whether the user changed it (else it fits).
    views: HashMap<FlowId, (View, bool)>,
    /// Each flow's boxes, moving under the forces (FLOW-4).
    sims: HashMap<FlowId, flow_model::Sim>,
    /// The box being dragged and where its corner is, in diagram units.
    held: Option<(FlowId, Node, Pt)>,
    /// The line whose messages are shown.
    selected: Option<LineKey>,
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
        id: FlowId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(flow) = self.snapshot.flow(id) else {
            return div().into_any_element();
        };
        let conversations = self.snapshot.conversations_of(flow);
        let model = flow_model::build(&conversations, &self.snapshot.transcripts);
        // The forces keep acting: a box let go bounces back, or finds a new
        // place when dragged far enough (FLOW-6). A frame is asked for as
        // long as something moves.
        let held = self
            .flow
            .held
            .filter(|(flow, _, _)| *flow == id)
            .map(|(_, node, at)| (node, at));
        let sim = self
            .flow
            .sims
            .entry(id)
            .or_insert_with(|| flow_model::Sim::new(&model));
        sim.update(&model);
        if held.is_some() || sim.moving() {
            if cx.reduce_motion() {
                sim.settle();
            } else if sim.tick(held) {
                window.request_animation_frame();
            }
        }
        let pos = sim.positions();
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
        // The arrows between two boxes go as one bundle, under one label;
        // those of one kind going both ways share one line.
        let drawing = flow_model::draw(&model, &rects);
        let carried = |line: &flow_model::Line| -> Vec<&Edge> {
            line.edges.iter().map(|e| &model.edges[*e]).collect()
        };
        // A line is picked by its first arrow.
        let key = |line: &flow_model::Line| {
            let first = &model.edges[line.edges[0]];
            (id, first.from, first.to, first.kind)
        };
        let routes: Vec<(LineKey, bool, Vec<Pt>)> = drawing
            .lines
            .iter()
            .map(|line| {
                let points = line.points.iter().map(|p| view.to_screen(*p)).collect();
                (key(line), line.both(), points)
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
                    for (key, both, points) in &routes {
                        let arrow = Arrow {
                            kind: key.3,
                            both: *both,
                            wide: selected == Some(*key),
                            scale: view.scale,
                        };
                        arrow.paint(window, bounds.origin, points);
                    }
                },
            )
            .absolute()
            .size_full()
        };
        // One small label per bundle: a count per line, in its colour (the
        // legend says which), each opening its messages.
        let s = view.scale.max(0.7);
        let labels = drawing
            .labels
            .iter()
            .enumerate()
            .map(|(ix, (spot, lines))| {
                let at = view.to_screen(*spot);
                let chips = lines.iter().map(|l| {
                    let line = &drawing.lines[*l];
                    let key = key(line);
                    let on = selected == Some(key);
                    let c = color(key.3);
                    div()
                        .id(SharedString::from(format!("flow-label-{ix}-{l}")))
                        .px(px(4. * s))
                        .rounded_full()
                        .bg(if on {
                            c
                        } else {
                            gpui::transparent_black().into()
                        })
                        .text_color(if on { theme::window() } else { c })
                        .cursor_pointer()
                        .hover(|st| st.bg(theme::surface_2()))
                        .child(flow_model::count(&carried(line)).to_string())
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.flow.selected = if this.flow.selected == Some(key) {
                                None
                            } else {
                                Some(key)
                            };
                            cx.notify();
                        }))
                });
                div()
                    .absolute()
                    .left(px(at.x - 140.))
                    .top(px(at.y - 8. * s))
                    .w(px(280.))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(1. * s))
                            .px(px(1. * s))
                            .py(px(0.5 * s))
                            .rounded_full()
                            .border_1()
                            .border_color(theme::border_strong())
                            .bg(theme::surface())
                            .text_size(px(10. * s))
                            .whitespace_nowrap()
                            .children(chips),
                    )
            })
            .collect::<Vec<_>>();
        let review = to_review(&model, &conversations, flow.flow.unread);
        let boxes: Vec<AnyElement> = model
            .nodes
            .iter()
            .filter_map(|node| {
                let r = rects.get(node)?;
                let marks = (review.contains(node), model.talked_to(*node));
                Some(self.flow_box(id, *node, *r, view, (&conversations, marks), cx))
            })
            .collect();
        // The picked line's arrows: one way, or both.
        let inspector = selected.and_then(|(_, a, b, kind)| {
            let edges: Vec<&Edge> = model
                .edges
                .iter()
                .filter(|e| e.kind == kind && [(a, b), (b, a)].contains(&(e.from, e.to)))
                .collect();
            (!edges.is_empty()).then(|| self.flow_inspector(id, &edges, cx))
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
            .child(self.flow_totals(&conversations))
            .child(Self::new_conversation_button(id, cx))
            .child(legend())
            .children(inspector)
            .into_any_element()
    }

    fn flow_box(
        &self,
        id: FlowId,
        node: Node,
        r: Rect,
        view: View,
        (conversations, (review, talked)): (&[&ConversationView], (bool, bool)),
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
                        .sims
                        .get(&id)
                        .and_then(|sim| sim.positions().get(&node).copied())
                        .unwrap_or(Pt { x: r.x, y: r.y });
                    this.flow.drag = Some(Drag::Box {
                        node,
                        from: pt(e.position),
                        start,
                        moved: false,
                    });
                }),
            );
        let Node::Conversation(sid) = node else {
            let first = conversations
                .first()
                .and_then(|f| self.snapshot.transcripts.get(&f.conversation.id))
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
        let Some(conversation) = conversations.iter().find(|x| x.conversation.id == sid) else {
            return shell.into_any_element();
        };
        let settings = &conversation.conversation.settings;
        let waiting_for: Vec<String> = self.waited(conversation).map(|x| x.name.clone()).collect();
        // What it asks of the user, as the left pane shows it (NAV-5).
        let attention = leon_domain::Attention::of(conversation.status, review);
        let (messages, tools) = self
            .snapshot
            .transcripts
            .get(&sid)
            .map_or((0, 0), |t| leon_domain::activity(&t.entries));
        let (model, effort) = self.model_and_effort(settings);
        let model = format!("{model} · effort {effort}");
        // Where it works: the repository and branch of its worktree, else
        // of the checkout its CLI runs in (CTX-2).
        let place = {
            let folder = conversation
                .folder
                .as_ref()
                .unwrap_or(&conversation.conversation.working_dir);
            let git = self.snapshot.git.get(folder);
            let repo = crate::right_folders::folder_name(folder, git);
            Some(match git.and_then(|g| g.branch.as_deref()) {
                Some(branch) => format!("{repo} · {branch}"),
                None => repo,
            })
        };
        let agent = crate::agent_color::agent_color(conversations, conversation);
        let circles = conversation.status == leon_domain::Status::Running && !cx.reduce_motion();
        let unread = conversation.conversation.unread;
        shell
            // Its right-click menu: mark it unread (NAV-4).
            .when(!unread, |d| {
                d.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.row_menu = Some((MenuFor::Conversation(sid), e.position));
                        cx.notify();
                    }),
                )
            })
            .rounded(px(10. * s))
            .line_height(gpui::relative(1.25))
            .bg(theme::surface())
            // Room for the border, drawn over the box below.
            .p(px(10. * s + 2.))
            .flex()
            .flex_col()
            .gap(px(3. * s))
            .cursor_pointer()
            // The agent's colour, as a band along the top, its corners
            // following the box's; going round the box while it works.
            .when(!circles, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(2.))
                        .left(px(2.))
                        .right(px(2.))
                        .h(px(4. * s))
                        // The box's inner curve: its radius less its border.
                        .rounded_t(px((10. * s - 2.).max(0.)))
                        .bg(agent),
                )
            })
            // The border, as a layer: GPUI paints a box's own border over
            // its children, which would hide the line below.
            .child(edge(s).border_2().border_color(theme::border_strong()))
            // What needs the user: the left pane's coloured line, over the
            // border's left side and following its curve.
            .when_some(crate::flow_row::attention_edge(attention), |d, color| {
                d.child(edge(s).border_l(px(3. * s)).border_color(color))
            })
            .when(circles, |d| {
                d.child(crate::flow_paint::circling(
                    SharedString::from(format!("circling-{}", sid.0)),
                    agent,
                    10. * s,
                    (4. * s).max(2.),
                ))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6. * s))
                    .text_size(px(13. * s))
                    .font_weight(FontWeight::SEMIBOLD)
                    // The left pane's icon, the green check for an answer
                    // to read; none while its colour goes round the box,
                    // the loader when motion is reduced.
                    .children(match attention {
                        leon_domain::Attention::Working if circles => None,
                        leon_domain::Attention::Working => Some(status_mark(
                            SharedString::from(format!("spin-flow-{}", sid.0)),
                            conversation.status,
                        )),
                        _ => Some(
                            crate::flow_row::attention_icon(attention)
                                .size(px(14. * s))
                                .flex_none()
                                .into_any_element(),
                        ),
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(agent)
                            .child(conversation.name.clone()),
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
            .children(place.map(|place| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(5. * s))
                    .font_family(theme::MONO)
                    .text_size(px(11. * s))
                    .text_color(theme::fg_muted())
                    .child(
                        crate::icons::folder(false, theme::fg_muted(), 11.)
                            .size(px(11. * s))
                            .flex_none(),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(place),
                    )
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6. * s))
                    .text_size(px(12. * s))
                    .text_color(match conversation.status {
                        // Still at work, through the agents it waits for.
                        leon_domain::Status::Running => theme::accent(),
                        leon_domain::Status::Idle if !waiting_for.is_empty() => theme::accent(),
                        leon_domain::Status::NeedsInput => theme::warn(),
                        leon_domain::Status::Error => theme::err(),
                        leon_domain::Status::Idle => theme::fg_muted(),
                    })
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(status_text(conversation, &waiting_for)),
                    )
                    // Which agent has an answer or a question for the user,
                    // as the left pane says it of the flow (NAV-5).
                    .children(crate::flow_row::attention_pill(attention, s)),
            )
            // Its activity, and "You" at the bottom right.
            .child(
                div()
                    .mt_auto()
                    .flex()
                    .items_center()
                    .gap(px(6. * s))
                    .text_size(px(11. * s))
                    .text_color(theme::fg_faint())
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child({
                                // The plan's progress, when the agent keeps one.
                                let done = conversation
                                    .plan
                                    .iter()
                                    .filter(|p| p.status == leon_domain::StepStatus::Done)
                                    .count();
                                let plan = if conversation.plan.is_empty() {
                                    String::new()
                                } else {
                                    format!(" · plan {done}/{}", conversation.plan.len())
                                };
                                format!("{messages} messages · {tools} tool calls{plan}")
                            }),
                    )
                    .child(div().flex_1())
                    .when(talked, |d| d.child(you_chip(s))),
            )
            .into_any_element()
    }

    fn flow_drag(&mut self, id: FlowId, e: &MouseMoveEvent, cx: &mut Context<Self>) {
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
                    self.flow.held = Some((id, node, at));
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

    /// End of a drag. A box that did not move was clicked: open its conversation.
    /// A click on the background (no pan) closes the message panel.
    fn flow_release(&mut self, id: FlowId, at: Pt, cx: &mut Context<Self>) {
        if let Some(Drag::Pan { from, .. }) = self.flow.drag
            && (at.x - from.x).abs() + (at.y - from.y).abs() <= 3.
        {
            self.flow.selected = None;
        }
        if let Some(Drag::Box {
            node: Node::Conversation(conversation),
            moved: false,
            ..
        }) = self.flow.drag
        {
            self.tabs.insert(id, Tab::Conversation(conversation));
        }
        // Let go: the forces take it from here.
        self.flow.held = None;
        self.flow.drag = None;
        cx.notify();
    }

    fn flow_zoom(&mut self, id: FlowId, at: Pt, factor: f32, cx: &mut Context<Self>) {
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

    /// The whole flow at a glance (FLOW-4): its conversations, their
    /// messages and tool calls, and what they used, added up.
    fn flow_totals(&self, conversations: &[&ConversationView]) -> AnyElement {
        let (mut messages, mut tools, mut usage) = (0, 0, leon_domain::Usage::nothing());
        for c in conversations {
            if let Some(t) = self.snapshot.transcripts.get(&c.conversation.id) {
                let (m, k) = leon_domain::activity(&t.entries);
                messages += m;
                tools += k;
                usage = usage.plus(leon_domain::Usage::total(&t.entries));
            }
        }
        let n = conversations.len();
        let running = conversations
            .iter()
            .filter(|c| c.status == leon_domain::Status::Running)
            .count();
        let part = |text: String, color: Rgba| div().text_color(color).child(text);
        // Wraps short of the "+ New conversation" button, top right.
        div()
            .absolute()
            .left_3()
            .top_3()
            .max_w(relative(0.62))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x_3()
            .gap_y_1()
            .px_3()
            .py(px(6.))
            .rounded_lg()
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .text_size(px(12.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(part(
                format!("{n} conversation{}", if n == 1 { "" } else { "s" }),
                theme::fg(),
            ))
            .when(running > 0, |d| {
                d.child(part(format!("{running} working"), theme::accent()))
            })
            .child(part(format!("{messages} messages"), theme::fg_muted()))
            .child(part(format!("{tools} tool calls"), theme::fg_muted()))
            .when(usage.input_tokens + usage.output_tokens > 0, |d| {
                d.child(part(
                    crate::transcript::usage_text(usage),
                    theme::fg_faint(),
                ))
            })
            .into_any_element()
    }

    /// Starts another conversation in this flow (FLOW-7), in plain sight.
    fn new_conversation_button(id: FlowId, cx: &mut Context<Self>) -> AnyElement {
        div()
            .absolute()
            .right_3()
            .top_3()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(primary(
                "flow-new-conversation",
                "+ New conversation",
                cx.listener(move |this, _, _, cx| this.open_new_in_flow(id, cx)),
            ))
            .into_any_element()
    }

    fn flow_controls(&self, id: FlowId, view: View, cx: &mut Context<Self>) -> AnyElement {
        let center = self
            .flow
            .area
            .get()
            .map_or(Pt { x: 450., y: 320. }, |b| Pt {
                x: f32::from(b.origin.x + b.size.width / 2.),
                y: f32::from(b.origin.y + b.size.height / 2.),
            });
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
            // Back to where the forces first settled it, after a box was
            // dragged into a new place.
            .child(button(
                "flow-reset-layout",
                "Reset layout",
                cx.listener(move |this, _, _, cx| {
                    this.flow.sims.remove(&id);
                    cx.notify();
                }),
            ))
            .into_any_element()
    }

    fn flow_inspector(&self, id: FlowId, edges: &[&Edge], cx: &mut Context<Self>) -> AnyElement {
        let edge = edges[0];
        let both = edges.len() > 1;
        let name = |node: Node| match node {
            Node::You => "You".to_owned(),
            Node::Conversation(s) => self
                .snapshot
                .conversations
                .get(&s)
                .map_or_default(|v| v.name.clone()),
        };
        let agent: Option<ConversationId> = match (edge.kind, edge.from, edge.to) {
            (EdgeKind::Prompt, _, Node::Conversation(s)) | (_, Node::Conversation(s), _) => Some(s),
            _ => None,
        };
        // Every message, in order; who sent it when they go both ways.
        let mut messages: Vec<(&Edge, &flow_model::Item)> = edges
            .iter()
            .flat_map(|e| e.items.iter().map(move |item| (*e, item)))
            .collect();
        messages.sort_by_key(|(_, item)| item.at);
        let items = messages.into_iter().enumerate().map(|(ix, (by, item))| {
            let sent = if both {
                format!("{} → {} · ", name(by.from), name(by.to))
            } else {
                String::new()
            };
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
                        .child(format!("{sent}{}", item.label)),
                )
                .child(crate::markdown::markdown(
                    SharedString::from(format!("flow-item-{ix}")),
                    &item.text,
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
                        "{} {} {}",
                        name(edge.from),
                        if both { "⇄" } else { "→" },
                        name(edge.to)
                    )))
                    .child(
                        div()
                            .text_color(theme::fg_muted())
                            .child(flow_model::label(edges)),
                    )
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
                    format!("Open {}", name(Node::Conversation(s))),
                    cx.listener(move |this, _, _, cx| {
                        this.tabs.insert(id, Tab::Conversation(s));
                        cx.notify();
                    }),
                ))
            }))
            .into_any_element()
    }
}

/// The boxes whose answer waits to be read (NAV-4, FLOW-4): those marked so;
/// in a flow unread with none marked (an older flow, or one marked unread by
/// hand), the agent that answered you last.
fn to_review(
    model: &flow_model::FlowModel,
    conversations: &[&ConversationView],
    flow_unread: bool,
) -> Vec<Node> {
    let marked: Vec<Node> = conversations
        .iter()
        .filter(|c| c.conversation.unread)
        .map(|c| Node::Conversation(c.conversation.id))
        .collect();
    if !marked.is_empty() || !flow_unread {
        return marked;
    }
    model
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Report && e.to == Node::You)
        .max_by_key(|e| e.items.iter().map(|i| i.at).max().unwrap_or(0))
        .map(|e| e.from)
        .into_iter()
        .collect()
}

/// A layer over a whole box, following its corners, for a border.
/// "You", in the "You" box's green, on the box of a conversation the user
/// wrote to: it stands out from those only agents talk to (FLOW-4).
fn you_chip(scale: f32) -> gpui::Div {
    div()
        .flex_none()
        .px(px(5. * scale))
        .rounded(px(4.))
        // Filled, so it shows even zoomed out.
        .bg(theme::ok())
        .text_size(px(10.5 * scale))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::window())
        .child("You")
}

fn edge(scale: f32) -> gpui::Div {
    div().absolute().inset_0().rounded(px(10. * scale))
}
