//! Offscreen screenshots of the UI (feature `preview`), so the UI can be
//! reviewed without a screen: `cargo run -p leon-ui --example screenshots
//! --features preview -- <folder>`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{App, AppContext as _, Bounds, WindowBounds, WindowOptions, point, px, size};
use gpui_component::{Root, Theme, ThemeMode};
use leon_core::{CoreHandle, Intent};
use leon_domain::ConversationId;

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
}

impl Workspace {
    fn preview(&mut self, shot: &Shot, cx: &mut gpui::Context<Self>) {
        self.open_picker = None;
        match shot {
            Shot::Empty => self.screen = Screen::Nothing,
            Shot::Conversation(id) => {
                self.select(*id, cx);
                self.tabs.remove(id);
            }
            Shot::PickerOpen(id, picker) => {
                self.select(*id, cx);
                self.tabs.remove(id);
                self.open_picker = Some((*picker).into());
            }
            Shot::Flow(id) => {
                self.select(*id, cx);
                self.tabs.insert(*id, Tab::Flow);
            }
            Shot::NewConversation => self.show_new_conversation(cx),
        }
        self.core.send(Intent::View(None));
        cx.notify();
    }
}

/// Opens an offscreen window, saves each shot as `<out>/<name>.png`, then
/// quits the app.
pub fn capture(core: CoreHandle, shots: Vec<(String, Shot)>, out: PathBuf, cx: &mut App) {
    gpui_component::init(cx);
    Theme::change(ThemeMode::Dark, None, cx);
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
        let view = cx.new(|cx| Workspace::new(&core, Instant::now(), window, cx));
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
            view.update(cx, |view, cx| view.preview(&shot, cx));
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
