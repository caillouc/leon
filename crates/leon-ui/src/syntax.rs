//! Syntax colours for code in diffs (GIT-2, GIT-3, GH-3): tree-sitter, as
//! in Zed (I21), through gpui-component's highlighter and its dark theme.
//! Each side of a hunk is read as one piece of code (the old side: context
//! and removed lines; the new side: context and added lines), so a line
//! takes its colours from the lines around it, without the rest of the file.

use std::ops::Range;
use std::sync::{Arc, OnceLock};

use gpui::HighlightStyle;
use gpui_component::highlighter::{HighlightTheme, Language, SyntaxHighlighter};
use leon_domain::{Hunk, LineKind};
use ropey::Rope;

/// One line's colours: byte ranges of the line and their style.
pub(crate) type LineStyles = Vec<(Range<usize>, HighlightStyle)>;

/// The grammar's name for a file, from its extension (or its name, for a
/// `Makefile`); `None` without one.
pub(crate) fn language(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next()?;
    let extension = name
        .rsplit_once('.')
        .map_or(name, |(_, e)| e)
        .to_lowercase();
    // C, C++ and Make extensions the highlighter does not know.
    let known = match extension.as_str() {
        "h" => "c",
        "hpp" | "hh" | "hxx" | "cc" | "cxx" => "cpp",
        "mk" => "make",
        other => other,
    };
    let language = Language::from_str(known);
    (language != Language::Plain).then(|| language.name())
}

/// Each line's colours in `hunk`, in its order.
pub(crate) fn hunk_styles(language: &str, hunk: &Hunk) -> Vec<LineStyles> {
    let side = |old: bool| -> Vec<usize> {
        (0..hunk.lines.len())
            .filter(|&i| match hunk.lines[i].kind {
                LineKind::Context => true,
                LineKind::Removed => old,
                LineKind::Added => !old,
            })
            .collect()
    };
    let mut styles: Vec<LineStyles> = vec![Vec::new(); hunk.lines.len()];
    for old in [true, false] {
        let lines = side(old);
        let texts: Vec<&str> = lines.iter().map(|&i| hunk.lines[i].text.as_str()).collect();
        for (k, line_styles) in highlight(language, &texts).into_iter().enumerate() {
            let i = lines[k];
            // Context lines take the new side's colours.
            if old && hunk.lines[i].kind == LineKind::Context {
                continue;
            }
            styles[i] = line_styles;
        }
    }
    styles
}

/// Colours of `lines` read as one piece of `language` code, a list per line.
fn highlight(language: &str, lines: &[&str]) -> Vec<LineStyles> {
    let text = lines.join("\n");
    let mut highlighter = SyntaxHighlighter::new(language);
    highlighter.update(None, &Rope::from_str(&text), None);
    let styles = highlighter.styles(&(0..text.len()), &**theme());
    let mut starts = Vec::with_capacity(lines.len());
    let mut at = 0;
    for line in lines {
        starts.push(at);
        at += line.len() + 1;
    }
    let mut out: Vec<LineStyles> = vec![Vec::new(); lines.len()];
    for (range, style) in styles {
        // A style may span lines (a long string, a comment): cut it per line.
        let first = starts
            .partition_point(|&s| s <= range.start)
            .saturating_sub(1);
        for (i, &start) in starts.iter().enumerate().skip(first) {
            if start >= range.end {
                break;
            }
            let end = start + lines[i].len();
            let (from, to) = (range.start.max(start), range.end.min(end));
            if from < to {
                out[i].push((from - start..to - start, style));
            }
        }
    }
    out
}

fn theme() -> &'static Arc<HighlightTheme> {
    static THEME: OnceLock<Arc<HighlightTheme>> = OnceLock::new();
    THEME.get_or_init(|| github().unwrap_or_else(HighlightTheme::default_dark))
}

/// GitHub's code colours (its "prettylights", dark), by highlight name.
const GITHUB: [(&str, &str); 22] = [
    ("attribute", "#79c0ff"),
    ("boolean", "#79c0ff"),
    ("comment", "#9198a1"),
    ("comment.doc", "#9198a1"),
    ("constant", "#79c0ff"),
    ("constructor", "#d2a8ff"),
    ("enum", "#ffa657"),
    ("function", "#d2a8ff"),
    ("keyword", "#ff7b72"),
    ("label", "#ffa657"),
    ("number", "#79c0ff"),
    ("operator", "#ff7b72"),
    ("property", "#79c0ff"),
    ("string", "#a5d6ff"),
    ("string.escape", "#79c0ff"),
    ("string.regex", "#7ee787"),
    ("string.special", "#a5d6ff"),
    ("tag", "#7ee787"),
    ("title", "#d2a8ff"),
    ("type", "#ffa657"),
    ("variable", "#f0f6fc"),
    ("variable.special", "#79c0ff"),
];

/// gpui-component's dark theme with GitHub's code colours (design.md,
/// Colour); `None` if its shape ever changes.
fn github() -> Option<Arc<HighlightTheme>> {
    let mut theme = serde_json::to_value(&*HighlightTheme::default_dark()).ok()?;
    let syntax = theme.pointer_mut("/style/syntax")?.as_object_mut()?;
    for (name, color) in GITHUB {
        syntax.insert(name.to_owned(), serde_json::json!({ "color": color }));
    }
    serde_json::from_value(theme).ok().map(Arc::new)
}

#[cfg(test)]
mod tests {
    use leon_domain::{DiffLine, Hunk, LineKind};

    use super::{hunk_styles, language};

    #[test]
    fn a_file_names_its_language() {
        let cases = [
            ("src/main.rs", Some("rust")),
            ("app/models.py", Some("python")),
            ("web/api.ts", Some("typescript")),
            ("deploy/values.yml", Some("yaml")),
            ("src/io.h", Some("c")),
            ("src/io.cc", Some("cpp")),
            ("App.java", Some("java")),
            ("Main.scala", Some("scala")),
            ("Makefile", Some("make")),
            ("run.sh", Some("bash")),
            // Without a grammar here: plain.
            ("web/App.tsx", None),
            ("web/app.css", None),
            ("Cargo.lock", None),
            ("LICENSE", None),
        ];
        for (path, expected) in cases {
            assert_eq!(language(path), expected, "{path}");
        }
    }

    /// Code is coloured on both sides, each line from its own side.
    #[test]
    fn both_sides_of_a_hunk_are_coloured() {
        let line = |kind, text: &str| DiffLine {
            kind,
            old: None,
            new: None,
            text: text.into(),
        };
        let hunk = Hunk {
            header: "@@ -1,2 +1,2 @@".into(),
            lines: vec![
                line(LineKind::Context, "fn retry() {"),
                line(LineKind::Removed, "    let max = 30;"),
                line(LineKind::Added, "    let max = 900;"),
                line(LineKind::Context, "}"),
            ],
        };
        let styles = hunk_styles("rust", &hunk);
        assert_eq!(styles.len(), 4);
        // `fn` and `let` are keywords on every side.
        assert!(
            styles[0].iter().any(|(r, _)| *r == (0..2)),
            "{:?}",
            styles[0]
        );
        assert!(
            styles[1].iter().any(|(r, _)| *r == (4..7)),
            "{:?}",
            styles[1]
        );
        assert!(
            styles[2].iter().any(|(r, _)| *r == (4..7)),
            "{:?}",
            styles[2]
        );
    }

    #[test]
    fn code_takes_githubs_colours() {
        let theme = super::github().expect("GitHub's colours apply");
        let keyword = gpui_base_style(&theme, "keyword");
        assert_eq!(keyword, Some(gpui::Hsla::from(gpui::rgb(0x00ff_7b72))));
    }

    fn gpui_base_style(
        theme: &gpui_component::highlighter::HighlightTheme,
        name: &str,
    ) -> Option<gpui::Hsla> {
        theme.style(name).and_then(|s| s.color)
    }
}
