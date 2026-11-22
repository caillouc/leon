//! Colours of the single dark theme (QUAL-3): GitHub's Primer dark palette,
//! as GitHub Copilot's app uses it (design.md, Colour). Every colour in the
//! UI comes from here.

use gpui::{Rgba, rgb, rgba};

/// Window background.
pub fn window() -> Rgba {
    rgb(0x000d_1117)
}
/// Side panes background.
pub fn panel() -> Rgba {
    rgb(0x0015_1b23)
}
/// Raised surfaces: inputs, buttons, chips.
pub fn surface() -> Rgba {
    rgb(0x0021_2830)
}
/// Hovered rows and buttons.
pub fn surface_2() -> Rgba {
    rgb(0x0026_2c36)
}
/// Menus, pickers' lists and hover lists: darker than everything, with a
/// strong border, as GitHub's overlays.
pub fn overlay() -> Rgba {
    rgb(0x0001_0409)
}
/// Hairlines.
pub fn border() -> Rgba {
    rgba(0x3d44_4d99)
}
/// Stronger borders: cards, menus.
pub fn border_strong() -> Rgba {
    rgb(0x003d_444d)
}
/// Main text.
pub fn fg() -> Rgba {
    rgb(0x00f0_f6fc)
}
/// Secondary text.
pub fn fg_muted() -> Rgba {
    rgb(0x0091_98a1)
}
/// Hints, empty states, section labels.
pub fn fg_faint() -> Rgba {
    rgb(0x0065_6c76)
}
/// Accent: selection, primary buttons, running.
pub fn accent() -> Rgba {
    rgb(0x0044_93f8)
}
/// Accent wash behind selected rows.
pub fn accent_soft() -> Rgba {
    rgba(0x388b_fd26)
}
/// Text on accent backgrounds.
pub fn on_accent() -> Rgba {
    rgb(0x000d_1117)
}
/// Needs attention: questions, pending.
pub fn warn() -> Rgba {
    rgb(0x00d2_9922)
}
/// Errors, removed lines.
pub fn err() -> Rgba {
    rgb(0x00f8_5149)
}
/// Success, added lines, open pull requests.
pub fn ok() -> Rgba {
    // GitHub's success green (3fb950), a shade calmer.
    rgb(0x0042_a353)
}
/// Merged pull requests.
pub fn merged() -> Rgba {
    rgb(0x00ab_7df8)
}
/// Background of an added line: a clear green, not the muted one of
/// text, so it reads as green on the dark background.
pub fn add_bg() -> Rgba {
    rgba(0x2ea0_4333)
}
/// Line-number gutter of an added line.
pub fn add_gutter() -> Rgba {
    rgba(0x3fb9_5059)
}
/// Background of a removed line: a clear red, likewise.
pub fn del_bg() -> Rgba {
    rgba(0xf851_492e)
}
/// Line-number gutter of a removed line.
pub fn del_gutter() -> Rgba {
    rgba(0xf851_4959)
}

/// Width of the reading column: a transcript, its composer, an item's
/// details. Centred in its pane, wider in a wider one
/// ([`crate::widgets::centered`]).
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
        rgb(0x0039_c5cf),
        rgb(0x00db_6d28),
        rgb(0x00db_61a2),
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
        "RED" => rgb(0x00f7_794b),
        "ORANGE" => rgb(0x00ed_8326),
        "PINK" => rgb(0x00e5_7bb2),
        "PURPLE" => merged(),
        _ => return None,
    })
}

/// Dims the window behind a drawer: GitHub's overlay black.
pub fn scrim() -> Rgba {
    gpui::rgba(0x0104_0999)
}

/// An agent's colour in the flow diagram, by its place among the
/// flow's agents.
pub fn agent_color(index: usize) -> Rgba {
    // GitHub's label colours (dark).
    const PALETTE: [u32; 8] = [
        0x004d_a0ff, // blue
        0x00b6_87f7, // purple
        0x001c_b0ab, // teal
        0x00d3_910d, // yellow
        0x00e5_7bb2, // pink
        0x0041_b445, // green
        0x00ed_8326, // orange
        0x0007_ace4, // cyan
    ];
    rgb(PALETTE[index % PALETTE.len()])
}

/// A colour of [`agent_color`]'s for a name (a repository, a login): the
/// same name, the same colour, everywhere.
pub fn name_color(name: &str) -> Rgba {
    agent_color(name.bytes().map(usize::from).sum())
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
    theme.popover = hsla(overlay());
    theme.secondary = hsla(surface_2());
    theme.accent = hsla(surface_2());
    theme.caret = hsla(fg());
    theme.selection = hsla(Rgba {
        a: 0.35,
        ..accent()
    });
    // Tables in messages: the app's surfaces, a tinted header, rows apart.
    theme.table = hsla(surface());
    theme.table_even = hsla(panel());
    theme.table_head = hsla(surface_2());
    theme.table_head_foreground = hsla(fg());
    theme.table_hover = hsla(surface_2());
    theme.table_row_border = hsla(border());
    theme.table_active = hsla(accent_soft());
    theme.table_active_border = hsla(accent());
    // The base layer (markdown, tables) reads its own copy of the colours:
    // bring it in line, or tables keep the stock near-black surface.
    let tokens = theme.semantic_tokens();
    theme.apply_semantic_tokens(&tokens);
    gpui_component::Theme::sync_base(cx);
}

/// A size given in pixels at 100 %, which grows with the app's zoom
/// (SETTINGS-2): the window's rem is 16 px times the zoom. Text, icons,
/// spacing and widths use it; the flow diagram keeps pixels, under its own
/// zoom.
pub fn rem(size: f32) -> gpui::Rems {
    gpui::rems(size / 16.)
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
            // GitHub's coral and orange label colours.
            ("RED", Some(rgb(0x00f7_794b))),
            ("orange", Some(rgb(0x00ed_8326))),
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
