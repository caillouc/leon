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

/// Width of the reading column: a transcript, its composer, an item's
/// details. Centred in its pane.
pub const COLUMN: f32 = 860.;

/// Monospace font for code, paths and diffs.
pub const MONO: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "DejaVu Sans Mono"
};

/// A color from six hex digits (`d73a4a`), as GitHub gives labels.
pub fn hex(digits: &str) -> Option<Rgba> {
    let value = u32::from_str_radix(digits.trim_start_matches('#'), 16).ok()?;
    (digits.trim_start_matches('#').len() == 6).then(|| rgb(value))
}

/// Board columns' colours, in the colours pull requests use, as each name
/// means it: backlog grey (like a draft), to do blue, in progress amber, in
/// review or QA green (like a ready pull request), done or in production
/// purple (like merged). Two columns never share a colour: in board order,
/// each column first gets the colour its name means unless an earlier one
/// took it; the others then get the colours no column has. Never red: red means an
/// error. `columns` are (name, place on the board).
pub fn status_colors(columns: &[(&str, usize)]) -> Vec<Rgba> {
    let palette = [
        fg_muted(),
        accent(),
        warn(),
        ok(),
        merged(),
        rgb(0x0073_c6d9),
        rgb(0x00f0_9a4f),
        rgb(0x00e8_8fc4),
    ];
    let mut order: Vec<usize> = (0..columns.len()).collect();
    order.sort_by_key(|&i| columns[i].1);
    let mut colors: Vec<Option<Rgba>> = vec![None; columns.len()];
    // First the colours names mean, then the rest from what is left.
    for &i in &order {
        if let Some(color) = meaning(columns[i].0).filter(|c| !colors.contains(&Some(*c))) {
            colors[i] = Some(color);
        }
    }
    for &i in &order {
        if colors[i].is_none() {
            colors[i] = palette
                .iter()
                .copied()
                .find(|c| !colors.contains(&Some(*c)));
        }
    }
    colors
        .into_iter()
        .map(|c| c.unwrap_or_else(fg_muted))
        .collect()
}

/// The colour a column's name means, if any.
fn meaning(name: &str) -> Option<Rgba> {
    let name = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    Some(
        if has(&[
            "done", "closed", "merged", "shipped", "complete", "released", "prod", "deployed",
        ]) {
            merged()
        } else if has(&["review", "qa", "test", "validat"]) {
            ok()
        } else if has(&["progress", "doing", "wip", "working", "dev"]) {
            warn()
        } else if has(&["backlog", "icebox", "triage", "later", "someday"]) {
            fg_muted()
        } else if has(&["todo", "to do", "ready", "next", "planned", "selected"]) {
            accent()
        } else {
            return None;
        },
    )
}

/// A board option's GitHub colour (`BLUE`, `RED`…) in the app's palette.
/// Red shows as a deeper orange: red means an error.
pub fn github_color(name: &str) -> Option<Rgba> {
    Some(match name.to_ascii_uppercase().as_str() {
        "GRAY" => fg_muted(),
        "BLUE" => accent(),
        "GREEN" => ok(),
        "YELLOW" => warn(),
        "RED" => rgb(0x00e9_784e),
        "ORANGE" => rgb(0x00f0_9a4f),
        "PINK" => rgb(0x00e8_8fc4),
        "PURPLE" => merged(),
        _ => return None,
    })
}

/// Dims the window behind a drawer.
pub fn scrim() -> Rgba {
    gpui::rgba(0x0000_0099)
}

/// An agent's colour in the flow diagram, by its place among the
/// conversation's agents.
pub fn agent_color(index: usize) -> Rgba {
    const PALETTE: [u32; 8] = [
        0x007a_a2f7, // blue
        0x00bb_9af7, // purple
        0x0073_daca, // teal
        0x00e0_af68, // amber
        0x00f7_768e, // rose
        0x009e_ce6a, // green
        0x00ff_9e64, // orange
        0x007d_cfff, // cyan
    ];
    rgb(PALETTE[index % PALETTE.len()])
}

/// Gives the widget library (inputs, menus, scrollbars) Leon's colours:
/// a focused text box gets an accent edge instead of a grey wash.
pub fn install(cx: &mut gpui::App) {
    use gpui::Hsla;
    let theme = gpui_component::Theme::global_mut(cx);
    let hsla = |c: Rgba| Hsla::from(c);
    theme.background = hsla(window());
    theme.foreground = hsla(fg());
    theme.muted_foreground = hsla(fg_muted());
    theme.border = hsla(border());
    theme.input = hsla(border_strong());
    theme.ring = hsla(accent());
    theme.popover = hsla(surface());
    theme.secondary = hsla(surface_2());
    theme.accent = hsla(surface_2());
    theme.caret = hsla(fg());
    theme.selection = hsla(Rgba {
        a: 0.35,
        ..accent()
    });
}

#[cfg(test)]
mod tests {
    use super::{accent, err, fg_muted, github_color, merged, ok, rgb, status_colors, warn};

    #[test]
    fn board_columns_get_their_meaning_and_never_share_a_colour() {
        let one = |name: &str| status_colors(&[(name, 0)])[0];
        let cases = [
            ("Backlog", fg_muted()),
            ("Todo", accent()),
            ("Ready", accent()),
            ("In progress", warn()),
            ("In DEV", warn()),
            ("In review", ok()),
            ("In QA | to validate", ok()),
            ("Done", merged()),
            ("In PROD | tech to validate", merged()),
        ];
        for (name, colour) in cases {
            assert_eq!(one(name), colour, "{name}");
        }
        // A real board: "To do" and "In PROD" used to share blue.
        let board = [
            ("Backlog", 0),
            ("To do", 1),
            ("In DEV", 2),
            ("In progress", 3),
            ("In review", 4),
            ("In QA | to validate", 5),
            ("In PROD | tech to validate", 6),
            ("Blocked by legal", 7),
        ];
        let colors = status_colors(&board);
        for (i, a) in colors.iter().enumerate() {
            assert_ne!(*a, err(), "{}", board[i].0);
            for (j, b) in colors.iter().enumerate().skip(i + 1) {
                assert_ne!(a, b, "{} and {}", board[i].0, board[j].0);
            }
        }
        // The earlier column keeps the colour its name means.
        assert_eq!((colors[2], colors[4]), (warn(), ok()));
    }

    #[test]
    fn github_colours_map_to_the_palette_without_red() {
        let cases = [
            ("RED", Some(rgb(0x00e9_784e))),
            ("orange", Some(rgb(0x00f0_9a4f))),
            ("YELLOW", Some(warn())),
            ("BLUE", Some(accent())),
            ("GREEN", Some(ok())),
            ("PURPLE", Some(merged())),
            ("GRAY", Some(fg_muted())),
            ("TEAL", None),
        ];
        for (name, expected) in cases {
            assert_eq!(github_color(name), expected, "{name}");
            assert_ne!(github_color(name), Some(err()), "{name}");
        }
    }
}
