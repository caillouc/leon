//! Colours of the single dark theme (QUAL-3), same palette as the validated
//! mock. Every colour in the UI comes from here.

use gpui::{Rgba, rgb, rgba};

/// Window background.
pub fn window() -> Rgba {
    rgb(0x0015_181d)
}
/// Side panes background.
pub fn panel() -> Rgba {
    rgb(0x0019_1c22)
}
/// Raised surfaces: cards, inputs, menus.
pub fn surface() -> Rgba {
    rgb(0x001e_2229)
}
/// Hovered rows and buttons.
pub fn surface_2() -> Rgba {
    rgb(0x0026_2a33)
}
/// Hairlines.
pub fn border() -> Rgba {
    rgb(0x002a_2e37)
}
/// Stronger borders: cards, menus.
pub fn border_strong() -> Rgba {
    rgb(0x0038_3d48)
}
/// Main text.
pub fn fg() -> Rgba {
    rgb(0x00d8_dbe2)
}
/// Secondary text.
pub fn fg_muted() -> Rgba {
    rgb(0x0091_98a5)
}
/// Hints, empty states, section labels.
pub fn fg_faint() -> Rgba {
    rgb(0x0064_6b78)
}
/// Accent: selection, primary buttons, running.
pub fn accent() -> Rgba {
    rgb(0x007e_9cf2)
}
/// Accent wash behind selected rows.
pub fn accent_soft() -> Rgba {
    rgba(0x7e9c_f226)
}
/// Text on accent backgrounds.
pub fn on_accent() -> Rgba {
    rgb(0x000c_1120)
}
/// Needs attention: questions, pending.
pub fn warn() -> Rgba {
    rgb(0x00e0_ae55)
}
/// Errors, removed lines.
pub fn err() -> Rgba {
    rgb(0x00e6_727b)
}
/// Success, added lines, open pull requests.
pub fn ok() -> Rgba {
    rgb(0x006d_be86)
}
/// Merged pull requests.
pub fn merged() -> Rgba {
    rgb(0x00b1_8cf0)
}
/// Background of an added line.
pub fn add_bg() -> Rgba {
    rgba(0x6dbe_861f)
}
/// Line-number gutter of an added line.
pub fn add_gutter() -> Rgba {
    rgba(0x6dbe_8638)
}
/// Background of a removed line.
pub fn del_bg() -> Rgba {
    rgba(0xe672_7b1f)
}
/// Line-number gutter of a removed line.
pub fn del_gutter() -> Rgba {
    rgba(0xe672_7b38)
}

/// Monospace font for code, paths and diffs.
pub const MONO: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "DejaVu Sans Mono"
};
