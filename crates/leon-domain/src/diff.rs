//! Diffs as Leon shows them (GIT-1..4), independent of how git produced them.

use regex::Regex;

/// How a file changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    /// New file.
    Added,
    /// Changed file.
    Modified,
    /// Removed file.
    Deleted,
    /// Moved, possibly changed.
    Renamed,
    /// New file git does not track yet.
    Untracked,
}

/// One line of a hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Unchanged.
    Context,
    /// Added.
    Added,
    /// Removed.
    Removed,
}

/// A line of a diff, with its numbers in the old and new file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Context, added or removed.
    pub kind: LineKind,
    /// Line number in the old file.
    pub old: Option<u32>,
    /// Line number in the new file.
    pub new: Option<u32>,
    /// Text without the leading `+`, `-` or space.
    pub text: String,
}

/// A hunk: its `@@` header and its lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// The `@@ -a,b +c,d @@ …` line.
    pub header: String,
    /// Its lines.
    pub lines: Vec<DiffLine>,
}

/// The diff of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// Path relative to the repository root (the new path for a rename).
    pub path: String,
    /// How it changed.
    pub status: FileStatus,
    /// Lines added.
    pub added: u32,
    /// Lines removed.
    pub removed: u32,
    /// Binary file: no hunks.
    pub binary: bool,
    /// Its hunks.
    pub hunks: Vec<Hunk>,
}

/// One `diff.fold` pattern (GIT-4): `glob:` (the default) or `re:`.
#[derive(Debug, Clone)]
pub struct FoldPattern {
    source: String,
    regex: Regex,
}

impl FoldPattern {
    /// Parses a pattern; `None` when it is not a valid glob or regex.
    pub fn parse(pattern: &str) -> Option<Self> {
        let regex = match pattern.strip_prefix("re:") {
            Some(re) => Regex::new(re).ok()?,
            None => Regex::new(&glob_to_regex(
                pattern.strip_prefix("glob:").unwrap_or(pattern),
            ))
            .ok()?,
        };
        Some(Self {
            source: pattern.to_owned(),
            regex,
        })
    }

    /// The pattern as written.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Whether the repo-relative `path` matches.
    pub fn matches(&self, path: &str) -> bool {
        self.regex.is_match(path)
    }
}

/// The first pattern that folds `path`, if any.
pub fn fold_reason<'a>(patterns: &'a [FoldPattern], path: &str) -> Option<&'a FoldPattern> {
    patterns.iter().find(|p| p.matches(path))
}

fn glob_to_regex(glob: &str) -> String {
    let mut out = String::from("^");
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    out.push_str("(?:.*/)?");
                } else {
                    out.push_str(".*");
                }
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            other => out.push_str(&regex::escape(&other.to_string())),
        }
    }
    out.push('$');
    out
}

#[cfg(test)]
mod tests {
    use super::{FoldPattern, fold_reason};

    #[test]
    fn globs_and_regexes_match_repo_paths() {
        let cases = [
            ("glob:**/*.lock", "Cargo.lock", true),
            ("glob:**/*.lock", "crates/a/Cargo.lock", true),
            ("**/*.lock", "Cargo.toml", false),
            ("glob:*.lock", "crates/a/Cargo.lock", false),
            ("glob:**/package-lock.json", "web/package-lock.json", true),
            ("re:^generated/", "generated/api.rs", true),
            ("re:^generated/", "src/generated/api.rs", false),
            ("glob:docs/?.md", "docs/a.md", true),
        ];
        for (pattern, path, expected) in cases {
            assert_eq!(
                FoldPattern::parse(pattern).unwrap().matches(path),
                expected,
                "{pattern} on {path}"
            );
        }
    }

    #[test]
    fn invalid_regex_is_rejected() {
        assert!(FoldPattern::parse("re:(").is_none());
    }

    #[test]
    fn first_matching_pattern_is_the_reason() {
        let patterns: Vec<_> = ["glob:**/*.lock", "re:lock"]
            .iter()
            .filter_map(|p| FoldPattern::parse(p))
            .collect();
        assert_eq!(
            fold_reason(&patterns, "Cargo.lock").map(FoldPattern::source),
            Some("glob:**/*.lock")
        );
        assert!(fold_reason(&patterns, "src/main.rs").is_none());
    }
}
