//! Pending review (GIT-5): comments on diff lines, sent as one message.

use crate::LineKind;

/// A comment on one line of a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewComment {
    /// Repo-relative path.
    pub path: String,
    /// Line number: in the new file, or in the old one for a removed line.
    pub line: u32,
    /// Kind of the commented line.
    pub kind: LineKind,
    /// The commented line, as quoted to the agent.
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
            let place = match c.kind {
                LineKind::Removed => format!("{} (removed line {})", c.path, c.line),
                LineKind::Added | LineKind::Context => format!("{}:{}", c.path, c.line),
            };
            format!("\n`{place}`\n> {}\n\n{}\n", c.code, c.text.trim())
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
    }
}
