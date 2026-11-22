//! Offscreen screenshots of the UI (feature `preview`), so the UI can be
//! reviewed without a screen: `cargo run -p leon-ui --example screenshots
//! --features preview -- <folder>`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Bounds, Focusable as _, Modifiers,
    MouseButton, MouseDownEvent, MouseMoveEvent, PlatformInput, WindowBounds, WindowOptions, point,
    px, size,
};
use gpui_component::{Root, Theme, ThemeMode};
use leon_core::{CoreHandle, Intent};
use leon_domain::FlowId;

use leon_domain::{LineKind, ReviewComment};

use crate::diff_view::{DiffSource, DraftAt};
use crate::github::GhList;
use crate::right::{Pending, RightTab};
use crate::workspace::{Screen, Tab, Workspace};

/// A UI state to capture.
#[derive(Debug, Clone)]
pub enum Shot {
    /// Nothing selected.
    Empty,
    /// A flow, on its first conversation.
    Conversation(FlowId),
    /// A flow, on one agent's tab.
    Agent(FlowId, leon_domain::ConversationId),
    /// A flow with one picker open (picker id, e.g. `conversation-model`).
    PickerOpen(FlowId, &'static str),
    /// A flow's Flow tab (its diagram).
    Flow(FlowId),
    /// The new-flow form.
    NewFlow,
    /// The new-flow form with this agent picked.
    NewFlowWith(&'static str),
    /// A flow with its form for another conversation open (FLOW-7).
    NewInFlow(FlowId),
    /// A flow with the right pane closed.
    RightClosed(FlowId),
    /// A flow with this text typed in its message box.
    Typed(FlowId, &'static str),
    /// A flow with a right-pane tab open: `changes`, `branch`, `pr`
    /// or `info`.
    Right(FlowId, &'static str),
    /// A flow's changes with a pending comment and one being written.
    Review(FlowId),
    /// My open pull requests in the main pane.
    MyPullRequests,
    /// My issues in the main pane, grouped by board status.
    MyIssues,
    /// The doctor's report.
    Doctor,
    /// Every flow's to-dos (TODO-3).
    Todos,
    /// A left-pane row's right-click menu.
    RowMenu(FlowId),
    /// A flow's name being edited in its left-pane row.
    Rename(FlowId),
    /// A to-do's when being edited on the To do page.
    EditTodo(leon_domain::TodoId),
    /// The Tools view.
    Tools,
    /// The Tools view while these CLIs are still being asked.
    ToolsAsking(Vec<leon_domain::BackendKind>),
    /// The Settings view.
    Settings,
    /// A flow's changed file, large in the drawer (path); side by
    /// side when `true`.
    Drawer(FlowId, &'static str, bool),
    /// An issue's or a pull request's details in the drawer.
    Detail(Box<leon_core::Detail>),
    /// A pull request's view with its changes, as GitHub gave them, open
    /// over it.
    PrDiff(Box<leon_core::Detail>, Box<leon_core::PrDiff>),
    /// A shot with the mouse pressed at the first point (in pixels) and
    /// moved through the others, a frame apart, still held: a drag under
    /// way. It ends without a drop.
    Drag(Box<Shot>, Vec<(f32, f32)>),
}

impl Workspace {
    fn open_first_conversation(&mut self, id: FlowId) {
        if let Some(first) = self.snapshot.flow(id).and_then(|c| c.conversations.first()) {
            self.tabs.insert(id, Tab::Conversation(*first));
        }
    }

    fn preview(&mut self, shot: &Shot, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
        self.open_picker = None;
        self.row_menu = None;
        self.right.draft = None;
        self.right.review.clear();
        self.right.tabs.clear();
        self.panes.right_open = true;
        self.right.drawer = None;
        crate::message_input::write_in(&self.composer, String::new(), window, cx);
        // A details drawer from an earlier shot does not stay open.
        if self.snapshot.detail.is_some() {
            self.snapshot = std::sync::Arc::new(leon_core::Snapshot {
                detail: None,
                ..(*self.snapshot).clone()
            });
        }
        match shot {
            Shot::Empty => self.screen = Screen::Nothing,
            Shot::Conversation(id) => {
                self.select(*id, cx);
                self.open_first_conversation(*id);
            }
            Shot::Agent(id, conversation) => {
                self.select(*id, cx);
                self.tabs.insert(*id, Tab::Conversation(*conversation));
            }
            Shot::PickerOpen(id, picker) => {
                self.select(*id, cx);
                self.open_first_conversation(*id);
                self.open_picker = Some((*picker).into());
            }
            Shot::RightClosed(id) => {
                self.select(*id, cx);
                self.open_first_conversation(*id);
                self.panes.right_open = false;
            }
            Shot::Typed(id, text) => {
                self.select(*id, cx);
                self.open_first_conversation(*id);
                crate::message_input::write_in(&self.composer, (*text).to_owned(), window, cx);
                self.composer.focus_handle(cx).focus(window, cx);
            }
            Shot::Flow(id) => {
                self.select(*id, cx);
                self.tabs.remove(id);
            }
            Shot::NewFlow => self.show_new_flow(cx),
            Shot::NewInFlow(id) => {
                self.select(*id, cx);
                self.open_new_in_flow(*id, cx);
            }
            Shot::NewFlowWith(agent) => {
                self.show_new_flow(cx);
                self.form.settings.agent = Some((*agent).to_owned());
            }
            Shot::Drawer(id, path, split) => {
                self.select(*id, cx);
                self.open_first_conversation(*id);
                self.right.split = *split;
                self.open_drawer(DiffSource::Changes, (*path).to_owned(), cx);
            }
            Shot::Right(id, tab) => {
                self.select(*id, cx);
                self.open_first_conversation(*id);
                let tab = match *tab {
                    "branch" => RightTab::Branch,
                    "info" => RightTab::Info,
                    _ => RightTab::Changes,
                };
                self.right.tabs.insert(*id, tab);
            }
            Shot::Review(id) => {
                self.select(*id, cx);
                self.open_first_conversation(*id);
                let at = |line: u32| DraftAt {
                    flow: *id,
                    source: DiffSource::Changes,
                    path: "src/webhooks/retry.rs".into(),
                    line,
                    end: line,
                    kind: LineKind::Added,
                    code: String::new(),
                    lines: ((0, 0), (0, 0)),
                };
                let first = at(12);
                self.right.review.insert(
                    *id,
                    vec![Pending {
                        source: first.source,
                        folder: self.folder(*id),
                        comment: ReviewComment {
                            path: first.path,
                            line: first.line,
                            end: first.end,
                            kind: first.kind,
                            code: first.code,
                            text: "15 minutes is long for a webhook; make it a setting?".into(),
                        },
                    }],
                );
                self.start_comment(at(14), window, cx);
            }
            Shot::MyPullRequests => self.screen = Screen::GitHub(GhList::PullRequests),
            Shot::MyIssues => self.screen = Screen::GitHub(GhList::Issues),
            Shot::PrDiff(detail, diff) => {
                self.screen = Screen::GitHub(GhList::PullRequests);
                self.snapshot = std::sync::Arc::new(leon_core::Snapshot {
                    detail: Some((**detail).clone()),
                    pr_diff: Some((**diff).clone()),
                    ..(*self.snapshot).clone()
                });
                self.right.drawer = Some(crate::diff_drawer::Drawer {
                    source: DiffSource::PullRequest,
                    scroll_to: None,
                });
            }
            Shot::Detail(detail) => {
                self.screen = Screen::GitHub(match detail.item().kind {
                    leon_domain::GhKind::Issue => GhList::Issues,
                    leon_domain::GhKind::PullRequest => GhList::PullRequests,
                });
                self.snapshot = std::sync::Arc::new(leon_core::Snapshot {
                    detail: Some((**detail).clone()),
                    ..(*self.snapshot).clone()
                });
            }
            Shot::Doctor => self.screen = Screen::Doctor,
            Shot::Todos => self.screen = Screen::Todos,
            Shot::Tools => self.screen = Screen::Tools,
            Shot::ToolsAsking(pending) => {
                self.screen = Screen::Tools;
                self.snapshot = std::sync::Arc::new(leon_core::Snapshot {
                    tools_pending: pending.clone(),
                    ..(*self.snapshot).clone()
                });
            }
            Shot::Settings => self.open_screen(Screen::Settings, window, cx),
            Shot::Drag(shot, _) => self.preview(shot, window, cx),
            Shot::Rename(id) => {
                self.select(*id, cx);
                self.start_rename(*id, window, cx);
            }
            Shot::EditTodo(id) => {
                self.screen = Screen::Todos;
                if let Some(todo) = self.snapshot.todos.iter().find(|t| t.id == *id).cloned() {
                    self.edit_todo_part(&todo, "page", crate::todos_view::Part::When, window, cx);
                }
            }
            Shot::RowMenu(id) => {
                self.select(*id, cx);
                self.row_menu = Some((
                    crate::row_menu::MenuFor::Flow(*id),
                    gpui::point(gpui::px(200.), gpui::px(520.)),
                ));
            }
        }
        self.core.send(Intent::View(None));
        cx.notify();
    }
}

/// Presses the mouse at `path`'s first point and moves it through the
/// others, a frame apart, as a hand would, without letting go.
async fn drag(window: AnyWindowHandle, path: &[(f32, f32)], cx: &mut AsyncApp) {
    let at = |(x, y): (f32, f32)| point(px(x), px(y));
    let Some((first, rest)) = path.split_first() else {
        return;
    };
    let down = PlatformInput::MouseDown(MouseDownEvent {
        button: MouseButton::Left,
        position: at(*first),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    let moves = rest.iter().map(|p| {
        PlatformInput::MouseMove(MouseMoveEvent {
            position: at(*p),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        })
    });
    for event in std::iter::once(down).chain(moves) {
        cx.update_window(window, |_, window, cx| {
            window.draw(cx).clear(cx);
            window.dispatch_event(event, cx);
        })
        .ok();
        cx.background_executor()
            .timer(Duration::from_millis(16))
            .await;
    }
}

/// Opens an offscreen window, saves each shot as `<out>/<name>.png`, then
/// quits the app.
pub fn capture(core: &CoreHandle, shots: Vec<(String, Shot)>, out: PathBuf, cx: &mut App) {
    gpui_component::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
    crate::theme::install(cx);
    let bounds = Bounds {
        origin: point(px(-10_000.), px(-10_000.)),
        size: size(px(1360.), px(860.)),
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        focus: false,
        show: true,
        ..WindowOptions::default()
    };
    let mut workspace = None;
    let opened = cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| Workspace::new(core, Instant::now(), window, cx));
        workspace = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx))
    });
    let (Ok(window), Some(view)) = (opened, workspace) else {
        tracing::error!("could not open the preview window");
        cx.quit();
        return;
    };
    cx.spawn(async move |cx| {
        for (name, shot) in shots {
            cx.update_window(window.into(), |_, window, cx| {
                view.update(cx, |view, cx| view.preview(&shot, window, cx));
                // A first frame: what sizes itself from its laid-out text
                // (a growing text box) settles by the shot's.
                window.draw(cx).clear(cx);
            })
            .ok();
            cx.background_executor().timer(Duration::from_millis(500)).await;
            if let Shot::Drag(_, path) = &shot {
                drag(window.into(), path, cx).await;
            }
            let image = cx.update_window(window.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
                let image = window.render_to_image();
                cx.stop_active_drag(window);
                image
            });
            match image {
                Ok(Ok(image)) => {
                    let path = out.join(format!("{name}.png"));
                    if let Err(error) = image.save(&path) {
                        tracing::error!(%error, path = %path.display(), "could not save the screenshot");
                    }
                }
                Ok(Err(error)) | Err(error) => tracing::error!(%error, shot = name, "could not render"),
            }
        }
        cx.update(|cx| cx.quit());
    })
    .detach();
}
