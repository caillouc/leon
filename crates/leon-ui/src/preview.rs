//! Offscreen screenshots of the UI (feature `preview`), so the UI can be
//! reviewed without a screen: `cargo run -p leon-ui --example screenshots
//! --features preview -- <folder>`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    App, AppContext as _, Bounds, Focusable as _, WindowBounds, WindowOptions, point, px, size,
};
use gpui_component::{Root, Theme, ThemeMode};
use leon_core::{CoreHandle, Intent};
use leon_domain::ConversationId;

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
    /// A conversation, on its first agent session.
    Conversation(ConversationId),
    /// A conversation with one picker open (picker id, e.g. `session-model`).
    PickerOpen(ConversationId, &'static str),
    /// A conversation's flow tab.
    Flow(ConversationId),
    /// The new-conversation form.
    NewConversation,
    /// The new-conversation form with this agent picked.
    NewConversationWith(&'static str),
    /// A flow with its form for another conversation open (FLOW-7).
    NewInFlow(ConversationId),
    /// A conversation with the right pane closed.
    RightClosed(ConversationId),
    /// A conversation with this text typed in its message box.
    Typed(ConversationId, &'static str),
    /// A conversation with a right-pane tab open: `changes`, `branch`, `pr`
    /// or `info`.
    Right(ConversationId, &'static str),
    /// A conversation's changes with a pending comment and one being written.
    Review(ConversationId),
    /// My open pull requests in the main pane.
    MyPullRequests,
    /// My issues in the main pane, grouped by board status.
    MyIssues,
    /// The doctor's report.
    Doctor,
    /// A left-pane row's right-click menu.
    RowMenu(ConversationId),
    /// The Tools view.
    Tools,
    /// The Tools view while these CLIs are still being asked.
    ToolsAsking(Vec<leon_domain::BackendKind>),
    /// The Settings view.
    Settings,
    /// A conversation's changed file, large in the drawer (path); side by
    /// side when `true`.
    Drawer(ConversationId, &'static str, bool),
    /// An issue's or a pull request's details in the drawer.
    Detail(Box<leon_core::Detail>),
}

impl Workspace {
    fn open_first_session(&mut self, id: ConversationId) {
        if let Some(first) = self
            .snapshot
            .conversation(id)
            .and_then(|c| c.sessions.first())
        {
            self.tabs.insert(id, Tab::Session(*first));
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
                self.open_first_session(*id);
            }
            Shot::PickerOpen(id, picker) => {
                self.select(*id, cx);
                self.open_first_session(*id);
                self.open_picker = Some((*picker).into());
            }
            Shot::RightClosed(id) => {
                self.select(*id, cx);
                self.open_first_session(*id);
                self.panes.right_open = false;
            }
            Shot::Typed(id, text) => {
                self.select(*id, cx);
                self.open_first_session(*id);
                crate::message_input::write_in(&self.composer, (*text).to_owned(), window, cx);
                self.composer.focus_handle(cx).focus(window, cx);
            }
            Shot::Flow(id) => {
                self.select(*id, cx);
                self.tabs.remove(id);
            }
            Shot::NewConversation => self.show_new_conversation(cx),
            Shot::NewInFlow(id) => {
                self.select(*id, cx);
                self.open_new_in_flow(*id, cx);
            }
            Shot::NewConversationWith(agent) => {
                self.show_new_conversation(cx);
                self.form.settings.agent = Some((*agent).to_owned());
            }
            Shot::Drawer(id, path, split) => {
                self.select(*id, cx);
                self.open_first_session(*id);
                self.right.split = *split;
                self.open_drawer(DiffSource::Changes, (*path).to_owned(), cx);
            }
            Shot::Right(id, tab) => {
                self.select(*id, cx);
                self.open_first_session(*id);
                let tab = match *tab {
                    "branch" => RightTab::Branch,
                    "pr" => RightTab::Pr,
                    "info" => RightTab::Info,
                    _ => RightTab::Changes,
                };
                self.right.tabs.insert(*id, tab);
            }
            Shot::Review(id) => {
                self.select(*id, cx);
                self.open_first_session(*id);
                let at = |line: u32| DraftAt {
                    conversation: *id,
                    source: DiffSource::Changes,
                    path: "src/webhooks/retry.rs".into(),
                    line,
                    end: line,
                    kind: LineKind::Added,
                    code: String::new(),
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
            Shot::Tools => self.screen = Screen::Tools,
            Shot::ToolsAsking(pending) => {
                self.screen = Screen::Tools;
                self.snapshot = std::sync::Arc::new(leon_core::Snapshot {
                    tools_pending: pending.clone(),
                    ..(*self.snapshot).clone()
                });
            }
            Shot::Settings => self.open_screen(Screen::Settings, window, cx),
            Shot::RowMenu(id) => {
                self.select(*id, cx);
                self.row_menu = Some((
                    crate::row_menu::MenuFor::Conversation(*id),
                    gpui::point(gpui::px(200.), gpui::px(520.)),
                ));
            }
        }
        self.core.send(Intent::View(None));
        cx.notify();
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
            })
            .ok();
            cx.background_executor().timer(Duration::from_millis(500)).await;
            let image = cx.update_window(window.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
                window.render_to_image()
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
