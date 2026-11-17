//! Leon's user interface (P1). The only crate that uses GPUI.
//!
//! Renders the core's snapshots and turns input into intents.
//!
//! Must not: do I/O, or hold business rules. Patterns to copy live in
//! `docs/ui-recipes.md`.

mod theme;
mod workspace;

use std::time::Instant;

use gpui::{App, AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use leon_core::CoreHandle;

use crate::workspace::Workspace;

/// Opens Leon's window and runs the UI until the user quits.
///
/// `started` is when the process started; the first frame logs the startup
/// time against it.
pub fn run(core: CoreHandle, started: Instant) {
    gpui_platform::application().run(move |cx: &mut App| {
        gpui_component::init(cx);
        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Leon".into()),
                ..TitlebarOptions::default()
            }),
            ..WindowOptions::default()
        };
        if let Err(error) = cx.open_window(options, |_window, cx| {
            cx.new(|cx| Workspace::new(&core, started, cx))
        }) {
            tracing::error!(%error, "could not open the window");
            cx.quit();
            return;
        }
        cx.on_window_closed(|cx, _window| cx.quit()).detach();
        cx.activate(true);
    });
}
