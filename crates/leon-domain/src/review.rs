//! Pending review (GIT-5): comments on diff lines, sent as one message.

use crate::LineKind;

/// A comment on one line of a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewComment {
    /// Repo-relative path.
    pub path: String,
    /// First line number: in the new file, or in the old one for a removed
    /// line.
    pub line: u32,
    /// Last line number, counted the same way; `line` for one line.
    pub end: u32,
    /// Kind of the last commented line.
    pub kind: LineKind,
    /// The commented lines, as quoted to the agent, one per line.
    pub code: String,
    /// What the user wrote.
    pub text: String,
}

/// The message that sends a whole review to an agent: one block per comment
/// with file, line, quoted code and comment, in the order they were written.
pub fn review_message(comments: &[ReviewComment]) -> String {
    let count = comments.len();
    let blocks: Vec<String> = comments
        .iter()
        .map(|c| {
            let lines = if c.end > c.line {
                format!("{}-{}", c.line, c.end)
            } else {
                c.line.to_string()
            };
            let place = match (c.kind, c.end > c.line) {
                (LineKind::Removed, false) => format!("{} (removed line {lines})", c.path),
                (LineKind::Removed, true) => format!("{} (removed lines {lines})", c.path),
                (LineKind::Added | LineKind::Context, _) => format!("{}:{lines}", c.path),
            };
            let quote: Vec<String> = c.code.lines().map(|l| format!("> {l}\n")).collect();
            format!("\n`{place}`\n{}\n{}\n", quote.concat(), c.text.trim())
        })
        .collect();
    format!(
        "**Review of the diff** ({count} comment{})\n{}",
        if count == 1 { "" } else { "s" },
        blocks.concat()
    )
}

#[cfg(test)]
mod tests {
    use super::{ReviewComment, review_message};
    use crate::LineKind;

    fn comment(kind: LineKind, line: u32, text: &str) -> ReviewComment {
        ReviewComment {
            path: "src/retry.rs".into(),
            line,
            end: line,
            kind,
            code: "let delay = base * 2;".into(),
            text: text.into(),
        }
    }

    #[test]
    fn a_review_quotes_each_line_with_its_comment() {
        let cases = [
            (
                vec![comment(LineKind::Added, 42, "Cap the delay. ")],
                "**Review of the diff** (1 comment)\n\n`src/retry.rs:42`\n> let delay = base * 2;\n\nCap the delay.\n",
            ),
            (
                vec![
                    comment(LineKind::Context, 3, "Why?"),
                    comment(LineKind::Removed, 9, "Keep this."),
                ],
                "**Review of the diff** (2 comments)\n\n`src/retry.rs:3`\n> let delay = base * 2;\n\nWhy?\n\n`src/retry.rs (removed line 9)`\n> let delay = base * 2;\n\nKeep this.\n",
            ),
        ];
        for (comments, expected) in cases {
            assert_eq!(review_message(&comments), expected);
        }
        let range = ReviewComment {
            end: 44,
            code: "let a = 1;\nlet b = 2;\nlet c = 3;".into(),
            ..comment(LineKind::Added, 42, "Merge these.")
        };
        assert_eq!(
            review_message(&[range]),
            "**Review of the diff** (1 comment)\n\n`src/retry.rs:42-44`\n> let a = 1;\n> let b = 2;\n> let c = 3;\n\nMerge these.\n"
        );
    }
}
