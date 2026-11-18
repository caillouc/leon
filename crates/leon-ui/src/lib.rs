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
mod conv_row;
mod conversation;
mod diff_drawer;
mod diff_rows;
mod diff_view;
mod doctor;
mod drawer;
mod flow;
mod flow_model;
mod flow_paint;
mod form;
mod gh_detail;
mod github;
mod icons;
mod left;
mod markdown;
mod message_input;
mod nav;
mod panes;
mod picker;
mod pr_page;
mod pr_view;
#[cfg(feature = "preview")]
pub mod preview;
mod right;
mod row_menu;
mod settings_agents;
mod settings_pickers;
mod settings_view;
mod shortcuts;
mod sprints;
mod theme;
mod tools_view;
mod transcript;
mod widgets;
mod workspace;

use std::time::Instant;

use gpui::{App, AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_component::{Root, Theme, ThemeMode};
use leon_core::CoreHandle;

pub use crate::icons::Assets;
use crate::workspace::Workspace;

/// Opens Leon's window and runs the UI until the user quits.
///
/// `started` is when the process started; the first frame logs the startup
/// time against it.
pub fn run(core: CoreHandle, started: Instant) {
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            shortcuts::install(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            theme::install(cx);
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
                // Shortcuts reach the workspace before anything is clicked.
                let focus = workspace.read(cx).focus.clone();
                focus.focus(window, cx);
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
