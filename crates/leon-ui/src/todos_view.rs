//! To-dos in the interface (TODO-1..TODO-4): the flow's in Info, with a box
//! to add one; every flow's on their own page, opened by a button at the
//! main pane's top right; how many are open on each flow's row.

use std::collections::HashSet;

use gpui::{
    AnyElement, Context, Entity, FontWeight, IntoElement, SharedString, Subscription, Window, div,
    prelude::*, relative,
};
use gpui_component::Sizable as _;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use leon_core::FlowView;
use leon_core::Intent;
use leon_domain::{FlowId, Todo, TodoId, open_todos, todo_flows};

use crate::drag_line::{self, DragLine, DraggedTodoFlow, Lead};
use crate::icons::{Icon, chevron, icon};
use crate::theme::{self, rem};
use crate::todo_undo::Did;
use crate::widgets::{HEADER_HEIGHT, centered, empty, label, pane_header};
use crate::workspace::{Screen, Workspace, value_of};

/// Room the to-dos button takes at the main pane's top right: the panes'
/// headers keep it free.
pub(crate) const BUTTON_ROOM: f32 = 72.;

/// The key of Info's To do section's fold: the same in every flow, kept
/// across restarts.
const INFO_FOLD: &str = "info:todos";

/// The most of a to-do's width its when takes, beside its what.
const WHEN_SHARE: f32 = 0.4;

/// What the to-dos remember between renders.
pub(crate) struct TodoState {
    /// The boxes that add one to the flow shown: when, what.
    pub when: Entity<InputState>,
    pub what: Entity<InputState>,
    /// The flows whose done to-dos are shown, in Info and on the page.
    pub show_done: HashSet<FlowId>,
    /// The latest tick or removal, while it can be undone.
    pub undo: crate::todo_undo::TodoUndo,
    /// The to-do part being edited, after a double click.
    pub editing: Option<Editing>,
    /// A flow's group being dragged on the To do page, and the place it
    /// would take: the page shows the groups in that order meanwhile.
    pub drag: Option<(FlowId, usize)>,
    /// Where a group shows on the page, all as wide: a dragged one's line
    /// takes that width.
    column: crate::widgets::Area,
}

/// A part of a to-do being edited in place: its box, and where it shows
/// (Info or the To do page).
pub(crate) struct Editing {
    id: TodoId,
    place: String,
    part: Part,
    input: Entity<TextareaState>,
    _subscription: Subscription,
}

/// A part of a to-do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    When,
    What,
}

impl TodoState {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> (Self, Vec<Subscription>) {
        let when = cx.new(|cx| InputState::new(window, cx).placeholder("When…"));
        let what = cx.new(|cx| InputState::new(window, cx).placeholder("What to do"));
        // Enter in either box adds it.
        let subscriptions = [&when, &what]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(input, window, |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.add_todo(window, cx);
                    }
                })
            })
            .collect();
        let state = Self {
            when,
            what,
            show_done: HashSet::new(),
            undo: crate::todo_undo::TodoUndo::default(),
            editing: None,
            drag: None,
            column: crate::widgets::Area::default(),
        };
        (state, subscriptions)
    }
}

impl Workspace {
    /// The boxes' text becomes a to-do of the flow shown: its what is
    /// needed, its when may be left out.
    fn add_todo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Flow(flow) = self.screen else {
            return;
        };
        let (when, what) = (
            value_of(&self.todos.when, cx),
            value_of(&self.todos.what, cx),
        );
        if what.trim().is_empty() {
            return;
        }
        self.core.send(Intent::AddTodo { flow, when, what });
        for input in [&self.todos.when, &self.todos.what] {
            input.update(cx, |state, cx| state.set_value("", window, cx));
        }
    }

    /// The flow's to-dos in Info (TODO-2): the open ones, the box to add
    /// one, then the done ones, folded under how many. Its heading folds
    /// all of it, in every flow, until unfolded.
    pub(crate) fn todo_section(&self, flow: FlowId, cx: &mut Context<Self>) -> AnyElement {
        let (done, open) = self.todos_of(flow);
        let folded = self.snapshot.folded_groups.contains(INFO_FOLD);
        // Its heading folds it; folded, how many are open still shows.
        let heading = crate::widgets::fold_toggle(INFO_FOLD.to_owned(), folded)
            .px_3()
            .pt_3()
            .pb_1()
            .child(label("To do").p_0())
            .when(folded && !open.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(rem(10.5))
                        .text_color(theme::fg_faint())
                        .child(open.len().to_string()),
                )
            });
        let section = div().px_1().flex().flex_col().child(heading);
        if folded {
            return section.into_any_element();
        }
        section
            .children(open.into_iter().map(|t| self.todo_row(t, "info", cx)))
            // Like a to-do: its empty box, then when → what.
            .child(
                div()
                    .px_3()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(icon(Icon::Pending, theme::fg_faint(), 16.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(relative(WHEN_SHARE))
                                    .child(Input::new(&self.todos.when).small()),
                            )
                            .child(div().flex_none().text_color(theme::fg_faint()).child("→"))
                            .child(div().flex_1().child(Input::new(&self.todos.what).small())),
                    ),
            )
            .children(self.done_todos(flow, &done, "info", cx))
            .into_any_element()
    }

    /// Flow `flow`'s to-dos: the done ones, then the open ones.
    fn todos_of(&self, flow: FlowId) -> (Vec<&Todo>, Vec<&Todo>) {
        self.snapshot
            .todos
            .iter()
            .filter(|t| t.flow == flow)
            .partition(|t| t.done)
    }

    /// A flow's done to-dos, folded under how many; `None` without any.
    fn done_todos(
        &self,
        flow: FlowId,
        done: &[&Todo],
        place: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if done.is_empty() {
            return None;
        }
        let shown = self.todos.show_done.contains(&flow);
        let toggle = div()
            .id(SharedString::from(format!("todos-done-{place}-{}", flow.0)))
            .mx_3()
            .mt_1()
            .flex()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .text_size(rem(12.))
            .text_color(theme::fg_muted())
            .hover(|s| s.text_color(theme::fg()))
            .child(chevron(shown, theme::fg_muted(), 14.))
            .child(format!("{} done", done.len()))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.todos.show_done.remove(&flow) {
                    this.todos.show_done.insert(flow);
                }
                cx.notify();
            }));
        Some(
            div()
                .flex()
                .flex_col()
                .child(toggle)
                .when(shown, |d| {
                    d.children(done.iter().map(|t| self.todo_row(t, place, cx)))
                })
                .into_any_element(),
        )
    }

    /// One to-do: its box, which marks it done or open again, its text, and
    /// on hover a red bin that removes it.
    fn todo_row(&self, todo: &Todo, place: &str, cx: &mut Context<Self>) -> AnyElement {
        let (id, done) = (todo.id, todo.done);
        let group = SharedString::from(format!("todo-{place}-{}", id.0));
        div()
            .id(group.clone())
            .group(group.clone())
            .px_3()
            .py(rem(6.))
            .rounded_lg()
            .flex()
            .items_start()
            .gap_3()
            .hover(|s| s.bg(theme::surface()))
            .child(check(id, done, place, cx))
            // When → what, side by side; done: struck out.
            .child(div().flex_1().min_w_0().child(if done {
                div()
                    .line_through()
                    .text_color(theme::fg_muted())
                    .child(todo.line())
                    .into_any_element()
            } else {
                self.when_what(todo, place, cx)
            }))
            .child(remove(id, group, place, cx))
            .into_any_element()
    }

    /// An open to-do as it reads: what it waits for, muted, an arrow, then
    /// what to do, in markdown with its line breaks kept; what alone when it
    /// waits for nothing. A double click on either part edits it in place.
    fn when_what(&self, todo: &Todo, place: &str, cx: &mut Context<Self>) -> AnyElement {
        let id = todo.id;
        let editing = self
            .todos
            .editing
            .as_ref()
            .filter(|e| e.id == id && e.place == place);
        let part = |part: Part, shown: AnyElement, cx: &mut Context<Self>| match editing {
            Some(e) if e.part == part => div()
                .w_full()
                .child(crate::widgets::in_place(
                    Textarea::new(&e.input).appearance(false),
                ))
                .into_any_element(),
            _ => {
                let todo = todo.clone();
                let place = place.to_owned();
                div()
                    .id(SharedString::from(format!(
                        "todo-{part:?}-{place}-{}",
                        id.0
                    )))
                    .min_w_0()
                    .max_w_full()
                    .cursor_text()
                    .child(shown)
                    .on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            if event.click_count() == 2 {
                                this.edit_todo_part(&todo, &place, part, window, cx);
                            }
                        }),
                    )
                    .into_any_element()
            }
        };
        let what = part(
            Part::What,
            crate::markdown::markdown(
                SharedString::from(format!("todo-what-{place}-{}", id.0)),
                &crate::markdown::hard_breaks(&todo.what),
            ),
            cx,
        );
        let what = div().flex_1().min_w_0().child(what);
        if todo.when.trim().is_empty() {
            return what.into_any_element();
        }
        let editing_when = editing.is_some_and(|e| e.part == Part::When);
        let when = part(
            Part::When,
            div()
                .text_color(theme::fg_muted())
                .child(todo.when.clone())
                .into_any_element(),
            cx,
        );
        // Side by side, each wrapping in its own column: the when as wide as
        // it reads, up to 40 %, the arrow on the first line, the what the
        // rest.
        div()
            .flex()
            .items_start()
            .gap_2()
            // What it waits for, in amber.
            .child(
                div()
                    .flex_none()
                    .pt(rem(3.))
                    .child(icon(Icon::Hourglass, theme::warn(), 13.)),
            )
            .child(
                div()
                    .min_w_0()
                    .max_w(relative(WHEN_SHARE))
                    .when(editing_when, |d| d.w(relative(WHEN_SHARE)))
                    .child(when),
            )
            .child(div().flex_none().text_color(theme::fg_faint()).child("→"))
            .child(what)
            .into_any_element()
    }

    /// Edits one part of to-do `todo` in place, in a box holding its text:
    /// Enter or leaving the box saves it, Esc gives up (TODO-1).
    pub(crate) fn edit_todo_part(
        &mut self,
        todo: &Todo,
        place: &str,
        part: Part,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = match part {
            Part::When => todo.when.clone(),
            Part::What => todo.what.clone(),
        };
        let input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .submit_on_enter(true);
            state.set_value(text, window, cx);
            state
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(
                event,
                InputEvent::PressEnter { shift: false, .. } | InputEvent::Blur
            ) {
                this.save_todo_edit(cx);
            }
        });
        gpui::Focusable::focus_handle(&input, cx).focus(window, cx);
        self.todos.editing = Some(Editing {
            id: todo.id,
            place: place.to_owned(),
            part,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Saves the part being edited, the other part as it was.
    fn save_todo_edit(&mut self, cx: &mut Context<Self>) {
        let Some(editing) = self.todos.editing.take() else {
            return;
        };
        let text = crate::workspace::text_of(&editing.input, cx);
        if let Some(todo) = self.snapshot.todos.iter().find(|t| t.id == editing.id) {
            let (when, what) = match editing.part {
                Part::When => (text, todo.what.clone()),
                Part::What => (todo.when.clone(), text),
            };
            self.core.send(Intent::EditTodo {
                id: editing.id,
                when,
                what,
            });
        }
        cx.notify();
    }

    /// The buttons at the main pane's top right: the to-dos', with how many
    /// are open in every flow, opening their page (TODO-3); then, on a
    /// flow while the right pane is closed, the one opening it again.
    pub(crate) fn todos_corner(&self, cx: &mut Context<Self>) -> AnyElement {
        let count = open_todos(&self.snapshot.todos, None);
        let here = self.screen == Screen::Todos;
        let reopen = matches!(self.screen, Screen::Flow(_)) && !self.panes.right_open;
        div()
            .absolute()
            .top(rem((HEADER_HEIGHT - 28.) / 2.))
            .right(rem(12.))
            .flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .id("todos-button")
                    .flex()
                    .items_center()
                    .gap(rem(5.))
                    .px_2()
                    .py(rem(4.))
                    .rounded_md()
                    .cursor_pointer()
                    .text_size(rem(12.5))
                    .text_color(theme::fg_muted())
                    .hover(|s| s.bg(theme::surface_2()).text_color(theme::fg()))
                    .when(here, |d| d.bg(theme::accent_soft()).text_color(theme::fg()))
                    .child(icon(Icon::Todo, theme::fg_muted(), 15.))
                    .when(count > 0, |d| d.child(count.to_string()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_screen(Screen::Todos, window, cx);
                    })),
            )
            .when(reopen, |d| {
                d.child(
                    div()
                        .id("right-open")
                        .p(rem(4.))
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::surface_2()))
                        .child(icon(Icon::Panel, theme::fg_muted(), 16.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.panes.right_open = true;
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    /// Every flow's to-dos, in the main pane (TODO-3): a group per flow with
    /// open ones, in the order the user dragged them to, the flow's name
    /// opening it, its done ones folded; a drag let go elsewhere changes
    /// nothing.
    pub(crate) fn render_todos(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if !cx.has_active_drag() {
            self.todos.drag = None;
        }
        let count = open_todos(&self.snapshot.todos, None);
        let order = todo_flows(
            self.snapshot.flows.iter().map(|f| &f.flow),
            &self.snapshot.todos,
        );
        let before: std::rc::Rc<[FlowId]> = order.clone().into();
        let groups: Vec<AnyElement> = drag_line::shown(&order, self.todos.drag)
            .into_iter()
            .filter_map(|id| self.snapshot.flow(id))
            .map(|view| self.todo_group(view, std::rc::Rc::clone(&before), cx))
            .collect();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(pane_header("To do", Some(format!("{count} open"))))
            .child(
                div()
                    .id("todos-page")
                    .flex_1()
                    .overflow_y_scroll()
                    .on_drop(drag_line::dropped::<DraggedTodoFlow, _>(
                        order,
                        |this| &mut this.todos.drag,
                        Intent::OrderTodoFlows,
                        cx,
                    ))
                    .child(centered(
                        880.,
                        div()
                            .px_6()
                            .py_5()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .when(groups.is_empty(), |d| {
                                d.child(empty("Nothing to do: every to-do is done."))
                            })
                            .children(groups),
                    )),
            )
            .into_any_element()
    }

    /// One flow's to-dos on the page (`order`: the page's before a drag):
    /// its title (opening it) and project, its open ones, its done ones
    /// folded. Dragged by its header among the others, its line, as wide as
    /// the group, follows the mouse, its place left empty, like a project's
    /// in the left pane.
    fn todo_group(
        &self,
        view: &FlowView,
        order: std::rc::Rc<[FlowId]>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let flow = view.flow.id;
        let dragged = self.todos.drag.is_some_and(|(id, _)| id == flow);
        let (done, open) = self.todos_of(flow);
        let project = self
            .snapshot
            .projects
            .iter()
            .find(|p| p.id == view.flow.project)
            .map(|p| p.name.clone());
        // Its project's colour, the one its repository has in the GitHub
        // lists.
        let color = theme::name_color(
            self.snapshot
                .project_repos
                .get(&view.flow.project)
                .map_or_else(|| project.as_deref().unwrap_or_default(), String::as_str),
        );
        // Its title opens it.
        let title = div()
            .id(SharedString::from(format!("todos-flow-{}", flow.0)))
            .flex()
            .items_baseline()
            .gap_2()
            .min_w_0()
            .cursor_pointer()
            .child(
                div()
                    .text_size(rem(13.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .hover(|s| s.text_color(theme::accent()))
                    .child(view.flow.title.clone()),
            )
            .children(project.map(|name| {
                div()
                    .text_size(rem(12.))
                    .text_color(theme::fg_faint())
                    .child(name)
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                // Not also a fold of the group.
                cx.stop_propagation();
                this.select(flow, cx);
            }))
            .into_any_element();
        let count = open.len();
        let mut rows: Vec<AnyElement> = Vec::new();
        if !dragged {
            rows.extend(open.into_iter().map(|t| self.todo_row(t, "page", cx)));
            rows.extend(self.done_todos(flow, &done, "page", cx));
        }
        let name = SharedString::from(view.flow.title.clone());
        let column = self.todos.column.clone();
        let group = self.group_with(
            format!("todos:{}", flow.0),
            color,
            title,
            count,
            rows,
            move |header| {
                header.when(dragged, |d| d.opacity(0.)).on_drag(
                    DraggedTodoFlow(flow),
                    move |_, grip, window, cx| {
                        let what = (Lead::Mark(color), name.clone(), count);
                        // The group's whole width, as it shows.
                        let width = column.get().map_or(grip.x, |b| b.size.width);
                        cx.new(|cx| DragLine::new(what, width, grip.x, window, cx))
                    },
                )
            },
        );
        div()
            .relative()
            .on_drag_move(drag_line::over(
                flow,
                order,
                |d: &DraggedTodoFlow| d.0,
                |this| &mut this.todos.drag,
                cx,
            ))
            .child(group)
            .child(crate::widgets::probe(self.todos.column.clone()))
            .into_any_element()
    }
}

/// A to-do's box: a click ticks an open one (done at once, with a note to
/// undo it), or opens a done one again.
fn check(id: TodoId, done: bool, place: &str, cx: &mut Context<Workspace>) -> AnyElement {
    let (glyph, color) = if done {
        (Icon::Passed, theme::ok())
    } else {
        (Icon::Pending, theme::fg_muted())
    };
    div()
        .id(SharedString::from(format!("todo-check-{place}-{}", id.0)))
        .flex_none()
        .pt(rem(2.))
        .cursor_pointer()
        .hover(|s| s.opacity(0.7))
        .child(icon(glyph, color, 16.))
        .on_click(cx.listener(move |this, _, _, cx| {
            if done {
                this.core.send(Intent::SetTodoDone(id, false));
            } else {
                this.do_todo(id, Did::Done, cx);
            }
        }))
        .into_any_element()
}

/// A to-do's small red bin, shown on hover: a click removes it at once,
/// with a note to undo it.
fn remove(id: TodoId, group: SharedString, place: &str, cx: &mut Context<Workspace>) -> AnyElement {
    div()
        .id(SharedString::from(format!("todo-remove-{place}-{}", id.0)))
        .flex_none()
        .rounded(rem(4.))
        .cursor_pointer()
        .p(rem(3.))
        .opacity(0.)
        .group_hover(group, |s| s.opacity(1.))
        .hover(|s| s.bg(theme::surface_2()))
        .child(icon(Icon::Trash, theme::err(), 13.))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.do_todo(id, Did::Removed, cx);
        }))
        .into_any_element()
}

/// How many to-dos of flow `flow` are open, for its row in the left pane
/// (TODO-4); `None` when none are.
pub(crate) fn todo_count(todos: &[Todo], flow: FlowId) -> Option<AnyElement> {
    let count = open_todos(todos, Some(flow));
    (count > 0).then(|| {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(rem(3.))
            .text_color(theme::fg_muted())
            .child(icon(Icon::Todo, theme::fg_muted(), 13.))
            .child(count.to_string())
            .into_any_element()
    })
}
