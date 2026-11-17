//! Leon's user interface (P1). The only crate that uses GPUI.
//!
//! Renders the core's snapshots and turns input into intents.
//!
//! Must not: do I/O, or hold business rules. Patterns to copy live in
//! `docs/ui-recipes.md`.

#![allow(
    clippy::too_many_lines,
    reason = "GPUI element trees are long builder chains; views are split by meaning, not by line count"
)]

mod composer;
mod conversation;
mod flow;
mod form;
mod left;
mod picker;
#[cfg(feature = "preview")]
pub mod preview;
mod right;
mod theme;
mod transcript;
mod widgets;
mod workspace;

use std::time::Instant;

use gpui::{App, AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_component::{Root, Theme, ThemeMode};
use leon_core::CoreHandle;

use crate::workspace::Workspace;

/// Opens Leon's window and runs the UI until the user quits.
///
/// `started` is when the process started; the first frame logs the startup
/// time against it.
pub fn run(core: CoreHandle, started: Instant) {
    gpui_platform::application().run(move |cx: &mut App| {
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        let bounds = Bounds::centered(None, size(px(1360.), px(860.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Leon".into()),
                ..TitlebarOptions::default()
            }),
            ..WindowOptions::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            let workspace = cx.new(|cx| Workspace::new(&core, started, window, cx));
            cx.new(|cx| Root::new(workspace, window, cx))
        });
        if let Err(error) = opened {
            tracing::error!(%error, "could not open the window");
            cx.quit();
            return;
        }
        cx.on_window_closed(|cx, _window| cx.quit()).detach();
        cx.activate(true);
    });
}
