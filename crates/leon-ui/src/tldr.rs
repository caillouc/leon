//! An answer's "TLDR" stands out (CONV-21): where a line starts with it,
//! outside code blocks, the word is drawn in the "To review" green, so the
//! conclusion is found at a glance. The markdown has no text colour, so the word is
//! drawn beside its paragraph's markdown.

use std::sync::Arc;

use gpui::{AnyElement, ElementId, FontWeight, IntoElement, div, prelude::*};

use crate::markdown::{fence, markdown};
use crate::theme;

/// The words drawn in green, as agents write them.
const WORDS: [&str; 2] = ["tldr", "tl;dr"];

/// A text cut at its first "TLDR" line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Tldr<'a> {
    /// What comes before its line.
    pub before: &'a str,
    /// The word, as written.
    pub word: &'a str,
    /// The rest of its paragraph, past the marks after the word (`:**`).
    pub rest: &'a str,
    /// What follows its paragraph.
    pub after: &'a str,
}

/// `text` cut at its first line starting with "TLDR" (or "TL;DR"), after
/// list, quote, heading or emphasis marks, outside code blocks; `None`
/// without one.
pub(crate) fn split_tldr(text: &str) -> Option<Tldr<'_>> {
    let mut fenced = false;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if fence(line).is_some() {
            fenced = !fenced;
        } else if !fenced && let Some((start, len)) = word_at_start(line) {
            let word_end = at + start + len;
            let end = paragraph_end(text, at + line.len());
            let rest = text[word_end..end]
                .trim_start_matches(|c: char| c.is_whitespace() || ":*_-–—".contains(c))
                .trim_end();
            return Some(Tldr {
                before: text[..at].trim_end(),
                word: &text[at + start..word_end],
                rest,
                after: text[end..].trim_start(),
            });
        }
        at += line.len();
    }
    None
}

/// Where the paragraph going on at `from` ends: its first blank line or
/// code block, else the text's end.
fn paragraph_end(text: &str, from: usize) -> usize {
    let mut at = from;
    for line in text[from..].split_inclusive('\n') {
        if line.trim().is_empty() || fence(line).is_some() {
            return at;
        }
        at += line.len();
    }
    text.len()
}

/// Where the word starts `line` and its length, past the marks that may
/// come first (`- `, `> `, `## `, `**`).
fn word_at_start(line: &str) -> Option<(usize, usize)> {
    let at = line.len()
        - line
            .trim_start_matches(|c: char| c.is_whitespace() || "-*_>#".contains(c))
            .len();
    let rest = &line[at..];
    WORDS.iter().find_map(|w| {
        let next = rest.get(w.len()..)?.chars().next();
        (rest.get(..w.len())?.eq_ignore_ascii_case(w) && !next.is_some_and(char::is_alphanumeric))
            .then_some((at, w.len()))
    })
}

/// The text around its TLDR as markdown, the word in green, its paragraph
/// beside it.
pub(crate) fn tldr_view(id: &ElementId, t: &Tldr<'_>) -> AnyElement {
    let part = |name: &'static str| ElementId::NamedChild(Arc::new(id.clone()), name.into());
    div()
        .flex()
        .flex_col()
        .gap_3()
        .when(!t.before.is_empty(), |d| {
            d.child(markdown(part("before"), t.before))
        })
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::BOLD)
                        // The "To review" pill's colour.
                        .text_color(theme::ok())
                        .child(t.word.to_owned()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(markdown(part("tldr"), t.rest)),
                ),
        )
        .when(!t.after.is_empty(), |d| {
            d.child(markdown(part("after"), t.after))
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Tldr, split_tldr};

    #[test]
    fn a_text_is_cut_at_its_tldr() {
        let tldr = |before, word, rest, after| {
            Some(Tldr {
                before,
                word,
                rest,
                after,
            })
        };
        let cases = [
            ("Done.", None),
            (
                "Fixed.\n\nTLDR: it works,\nmerge it.\n\nThanks.",
                tldr("Fixed.", "TLDR", "it works,\nmerge it.", "Thanks."),
            ),
            ("**TL;DR:** merge it", tldr("", "TL;DR", "merge it", "")),
            ("- Tldr — later", tldr("", "Tldr", "later", "")),
            // Not a word of its own, not at a line's start, not code.
            ("TLDRs are short", None),
            ("See the TLDR below", None),
            (
                "```\nTLDR: x\n```\nTLDR: y",
                tldr("```\nTLDR: x\n```", "TLDR", "y", ""),
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(split_tldr(text), expected, "{text}");
        }
    }
}
