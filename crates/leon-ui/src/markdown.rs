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

/// Markdown, selectable, its code blocks with a small corner.
pub(crate) fn markdown(id: impl Into<ElementId>, text: &str) -> AnyElement {
    TextView::markdown(id.into(), SharedString::from(text.to_owned()))
        .selectable(true)
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

/// Which folded sections are open, by id.
pub(crate) type OpenDetails = HashSet<SharedString>;

#[cfg(test)]
mod tests {
    use super::{Part, parts};

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
