//! Markdown as agents and GitHub write it: text, code blocks with a small
//! corner (language, copy), and GitHub's folded `<details>` sections, which
//! stay folded until clicked (a long list of passing tests…).

use std::collections::HashSet;

use gpui::{AnyElement, Context, ElementId, IntoElement, SharedString, div, prelude::*, px};
use gpui_component::text::TextView;

use crate::icons::{Icon, chevron, icon};
use crate::theme;
use crate::workspace::Workspace;

/// A piece of a text: plain markdown, or a folded section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    Text(String),
    Details { summary: String, body: String },
}

/// The text cut at its top-level `<details>` sections (nested ones stay in
/// their section's body). An unclosed section runs to the end.
pub(crate) fn parts(text: &str) -> Vec<Part> {
    // Tags are ASCII: lowering ASCII keeps every byte where it was.
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(open) = lower[at..].find("<details").map(|i| at + i) {
        let Some(tag_end) = lower[open..].find('>').map(|i| open + i + 1) else {
            break;
        };
        push_text(&mut out, &text[at..open]);
        let (inner_end, next) = closing(&lower, tag_end);
        let inner = &text[tag_end..inner_end];
        out.push(details(inner));
        at = next;
    }
    push_text(&mut out, &text[at..]);
    out
}

fn push_text(out: &mut Vec<Part>, text: &str) {
    if !text.trim().is_empty() {
        out.push(Part::Text(text.to_owned()));
    }
}

/// Where the section opened before `from` ends: the start of its
/// `</details>` and the position after it, nested sections counted.
fn closing(lower: &str, from: usize) -> (usize, usize) {
    let mut depth = 1;
    let mut at = from;
    loop {
        let open = lower[at..].find("<details").map(|i| at + i);
        let close = lower[at..].find("</details>").map(|i| at + i);
        match (open, close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                at = o + 1;
            }
            (_, Some(c)) => {
                depth -= 1;
                if depth == 0 {
                    return (c, c + "</details>".len());
                }
                at = c + 1;
            }
            (_, None) => return (lower.len(), lower.len()),
        }
    }
}

/// A section's summary (tags stripped) and body.
fn details(inner: &str) -> Part {
    let lower = inner.to_ascii_lowercase();
    let summary = lower.find("<summary").and_then(|s| {
        let start = s + lower[s..].find('>')? + 1;
        let end = start + lower[start..].find("</summary>")?;
        Some((start, end, end + "</summary>".len()))
    });
    match summary {
        Some((start, end, after)) => Part::Details {
            summary: strip_tags(&inner[start..end]),
            body: inner[after..].trim().to_owned(),
        },
        None => Part::Details {
            summary: "Details".to_owned(),
            body: inner.trim().to_owned(),
        },
    }
}

fn strip_tags(text: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_owned()
}

/// The core, reachable from a click on a link in any text.
pub(crate) struct Core(pub leon_core::CoreHandle);

impl gpui::Global for Core {}

/// A link: a GitHub issue or pull request opens Leon's view of it (GH-2,
/// GH-3), anything else the browser.
pub(crate) fn open_link(url: &str, cx: &mut gpui::App) {
    match (leon_domain::GhRef::from_url(url), cx.try_global::<Core>()) {
        (Some(item), Some(core)) => core.0.send(leon_core::Intent::ShowDetail(Some(item))),
        _ => cx.open_url(url),
    }
}

/// Markdown, selectable, its code blocks with a small corner, its GitHub
/// links opening Leon's view.
pub(crate) fn markdown(id: impl Into<ElementId>, text: &str) -> AnyElement {
    TextView::markdown(id.into(), SharedString::from(text.to_owned()))
        .selectable(true)
        .on_link_click(|url, _, _, cx| open_link(url, cx))
        // A small corner (language, copy) instead of the library's label,
        // which pushes the code down by a whole line.
        .code_block_actions(|code, _, _| code_corner(code.lang(), code.code()))
        .into_any_element()
}

/// The top-right corner of a code block: its language and a copy button.
fn code_corner(lang: Option<SharedString>, code: SharedString) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_1()
        .text_size(px(11.))
        .text_color(theme::fg_faint())
        .children(lang)
        .child(
            div()
                .id("copy")
                .p(px(3.))
                .rounded(px(4.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::surface_2()))
                .child(icon(Icon::Copy, theme::fg_muted(), 13.))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(code.to_string()));
                }),
        )
        .into_any_element()
}

impl Workspace {
    /// `text` as markdown, its `<details>` sections folded until clicked.
    /// `id` must be unique in the window.
    pub(crate) fn rich_markdown(&self, id: &str, text: &str, cx: &mut Context<Self>) -> AnyElement {
        let parts = parts(text);
        if let [Part::Text(only)] = parts.as_slice() {
            return markdown(SharedString::from(id.to_owned()), only);
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(parts.into_iter().enumerate().map(|(n, part)| match part {
                Part::Text(text) => markdown(SharedString::from(format!("{id}-{n}")), &text),
                Part::Details { summary, body } => {
                    let key = SharedString::from(format!("{id}-d{n}"));
                    let open = self.open_details.contains(&key);
                    let toggle = key.clone();
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(theme::border())
                        .child(
                            div()
                                .id(key.clone())
                                .px_2()
                                .py(px(5.))
                                .flex()
                                .items_center()
                                .gap_2()
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::surface_2()))
                                .child(chevron(open, theme::fg_muted(), 13.))
                                .child(div().text_color(theme::fg_muted()).child(summary))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.open_details.remove(&toggle) {
                                        this.open_details.insert(toggle.clone());
                                    }
                                    cx.notify();
                                })),
                        )
                        .when(open, |d| {
                            d.child(
                                div()
                                    .px_3()
                                    .py_2()
                                    .border_t_1()
                                    .border_color(theme::border())
                                    .child(self.rich_markdown(&key, &body, cx)),
                            )
                        })
                        .into_any_element()
                }
            }))
            .into_any_element()
    }
}

/// The start of `text` for a folded message (CONV-12): its first `n` lines
/// and about `chars` characters at most, a first line longer than that cut
/// at a space with "…". Never cutting a code block: the cut moves up to
/// just before the block, or, when the text starts with one, the block is
/// closed after its first lines.
pub(crate) fn first_lines(text: &str, n: usize, chars: usize) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut used = 0;
    for line in text.lines().take(n) {
        used += line.chars().count();
        if used > chars && !lines.is_empty() {
            break;
        }
        lines.push(line);
    }
    if let [only] = lines.as_slice()
        && only.chars().count() > chars
        && fence(only).is_none()
    {
        return shorten(only, chars);
    }
    // The code block still open at the cut: where it starts, its fence.
    let mut open: Option<(usize, &str)> = None;
    for (ix, line) in lines.iter().enumerate() {
        let Some(found) = fence(line) else { continue };
        open = match open {
            Some((_, start)) if closes(start, line) => None,
            None => Some((ix, found)),
            still => still,
        };
    }
    if let Some((start, mark)) = open {
        if lines[..start].iter().any(|l| !l.trim().is_empty()) {
            lines.truncate(start);
        } else {
            lines.push(mark);
        }
    }
    let end = lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(0, |ix| ix + 1);
    lines[..end].join("\n")
}

/// `line` cut to about `chars` characters, at a space when it has one, with
/// "…".
fn shorten(line: &str, chars: usize) -> String {
    let head: String = line.chars().take(chars).collect();
    let next = line.chars().nth(chars);
    let head = match (next, head.rfind(char::is_whitespace)) {
        (Some(c), Some(space)) if !c.is_whitespace() => &head[..space],
        _ => head.as_str(),
    };
    format!("{}…", head.trim_end())
}

/// The fence opening or closing a code block on `line`: three or more
/// backticks or tildes.
fn fence(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let mark = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = t.len() - t.trim_start_matches(mark).len();
    (run >= 3).then(|| &t[..run])
}

/// Whether `line` closes a block opened by `start`: the same character, at
/// least as many, and nothing after.
fn closes(start: &str, line: &str) -> bool {
    fence(line)
        .is_some_and(|f| f.starts_with(start) && line.trim_start()[f.len()..].trim().is_empty())
}

/// Which folded sections are open, by id.
pub(crate) type OpenDetails = HashSet<SharedString>;

#[cfg(test)]
mod tests {
    use super::{Part, first_lines, parts};

    #[test]
    fn a_fold_never_cuts_a_code_block() {
        let cases = [
            ("a\nb\nc", 2, "a\nb"),
            // Cut inside a block: before it, without the blank line.
            ("intro\n\n```diff\n+x\n+y\n```\nend", 4, "intro"),
            // A message starting with a block: its first lines, closed.
            (
                "```rust\nfn a\nfn b\nfn c\n```",
                3,
                "```rust\nfn a\nfn b\n```",
            ),
            // A block closed before the cut stays.
            ("a\n```\nx\n```\nb\nc", 5, "a\n```\nx\n```\nb"),
            // A shorter fence inside does not close a longer one.
            ("````md\n```\nx", 3, "````md\n```\nx\n````"),
            ("~~~\nx\ny", 2, "~~~\nx\n~~~"),
        ];
        for (text, n, shown) in cases {
            assert_eq!(first_lines(text, n, usize::MAX), shown, "{text:?}");
        }
        // A few long lines fold too: as many as fit, a too long first one
        // cut at a space.
        let chars = [
            ("one\ntwo\nthree", 7, "one\ntwo"),
            ("aaaa bbbb cccc", 9, "aaaa bbbb…"),
            ("aaaa bbbbbb", 7, "aaaa…"),
            ("aaaaaaaaaa", 4, "aaaa…"),
        ];
        for (text, budget, shown) in chars {
            assert_eq!(first_lines(text, 6, budget), shown, "{text:?}");
        }
    }

    #[test]
    fn details_sections_are_cut_out_folded() {
        let text = "## Tests\n\nAll green.\n\n<details><summary><b>42 passed</b></summary>\n\n- a ok\n- b ok\n</details>\n\nDone.";
        assert_eq!(
            parts(text),
            [
                Part::Text("## Tests\n\nAll green.\n\n".into()),
                Part::Details {
                    summary: "42 passed".into(),
                    body: "- a ok\n- b ok".into()
                },
                Part::Text("\n\nDone.".into()),
            ]
        );
        // Nested sections stay inside; an unclosed one runs to the end;
        // a section without a summary gets one.
        let nested = "<DETAILS open><summary>outer</summary>x<details><summary>in</summary>y</details>z</DETAILS>";
        assert_eq!(
            parts(nested),
            [Part::Details {
                summary: "outer".into(),
                body: "x<details><summary>in</summary>y</details>z".into()
            }]
        );
        assert_eq!(
            parts("<details>loose"),
            [Part::Details {
                summary: "Details".into(),
                body: "loose".into()
            }]
        );
        assert_eq!(parts("plain"), [Part::Text("plain".into())]);
    }
}
