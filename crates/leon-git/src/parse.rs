//! Parses `git diff` unified output into [`FileDiff`]s.
//!
//! Expects the `a/` and `b/` prefixes and no colour (the caller forces both).

use leon_domain::{DiffLine, FileDiff, FileStatus, Hunk, LineKind};

/// Every file of a `git diff` output, in git's order.
pub(crate) fn parse_diff(text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut cursor = Cursor::default();
    for line in text.split_terminator('\n') {
        if cursor.is_done()
            && let Some(names) = line.strip_prefix("diff --git ")
        {
            files.push(new_file(names));
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if line.starts_with('\\') {
            // "\ No newline at end of file": not a line of either file.
        } else if !cursor.is_done() {
            cursor.take(file, line);
        } else if line.starts_with("@@ ") {
            if let Some(opened) = Cursor::open(line) {
                cursor = opened;
                file.hunks.push(Hunk {
                    header: line.to_owned(),
                    lines: Vec::new(),
                });
            }
        } else {
            read_header(file, line);
        }
    }
    files
}

fn new_file(names: &str) -> FileDiff {
    FileDiff {
        path: header_path(names),
        status: FileStatus::Modified,
        added: 0,
        removed: 0,
        binary: false,
        hunks: Vec::new(),
    }
}

/// Applies one extended header line (between `diff --git` and the first
/// hunk). `---`, `+++`, `index`, mode and similarity lines change nothing.
fn read_header(file: &mut FileDiff, line: &str) {
    if line.starts_with("new file mode ") {
        file.status = FileStatus::Added;
    } else if line.starts_with("deleted file mode ") {
        file.status = FileStatus::Deleted;
    } else if let Some(to) = line.strip_prefix("rename to ") {
        file.status = FileStatus::Renamed;
        file.path = unquote(to);
    } else if line.starts_with("Binary files ") {
        file.binary = true;
    }
}

/// The path in `a/<path> b/<path>`. Both sides name the same file unless it
/// is a rename, whose new path comes from `rename to`. Falls back to the raw
/// text when the two sides differ.
fn header_path(names: &str) -> String {
    let mid = names.len() / 2;
    if names.len() % 2 == 1
        && names.as_bytes().get(mid) == Some(&b' ')
        && let (Some(a), Some(b)) = (names.get(..mid), names.get(mid + 1..))
    {
        let (a, b) = (unquote(a), unquote(b));
        if let (Some(old), Some(new)) = (a.strip_prefix("a/"), b.strip_prefix("b/"))
            && old == new
        {
            return new.to_owned();
        }
    }
    names.to_owned()
}

/// Escapes git uses in a quoted path, besides octal bytes.
const ESCAPES: [(u8, u8); 9] = [
    (b'a', 0x07),
    (b'b', 0x08),
    (b't', b'\t'),
    (b'n', b'\n'),
    (b'v', 0x0b),
    (b'f', 0x0c),
    (b'r', b'\r'),
    (b'"', b'"'),
    (b'\\', b'\\'),
];

/// Undoes git's C-style quoting of a path (`"a\tb"`); unquoted text is
/// returned as it is.
fn unquote(text: &str) -> String {
    let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) else {
        return text.to_owned();
    };
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        if byte != b'\\' {
            out.push(byte);
            i += 1;
            continue;
        }
        let next = bytes.get(i + 1).copied();
        let simple = ESCAPES.iter().find(|(c, _)| Some(*c) == next);
        if let Some(&(_, value)) = simple {
            out.push(value);
            i += 2;
        } else if let Some(value) = octal(bytes.get(i + 1..i + 4)) {
            out.push(value);
            i += 4;
        } else {
            out.push(byte);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn octal(digits: Option<&[u8]>) -> Option<u8> {
    let digits = std::str::from_utf8(digits?).ok()?;
    u8::from_str_radix(digits, 8).ok()
}

/// Position inside the current hunk: next line numbers and lines left.
#[derive(Debug, Default)]
struct Cursor {
    old: u32,
    new: u32,
    old_left: u32,
    new_left: u32,
}

impl Cursor {
    /// Opens a hunk from its `@@ -a,b +c,d @@` header.
    fn open(header: &str) -> Option<Self> {
        let ranges = header.strip_prefix("@@ -")?.split_once(" @@")?.0;
        let (old, new) = ranges.split_once(" +")?;
        let ((old, old_left), (new, new_left)) = (range(old)?, range(new)?);
        Some(Self {
            old,
            new,
            old_left,
            new_left,
        })
    }

    /// Every line announced by the header was read.
    fn is_done(&self) -> bool {
        self.old_left == 0 && self.new_left == 0
    }

    /// Adds one hunk line to `file`'s last hunk.
    fn take(&mut self, file: &mut FileDiff, line: &str) {
        let (kind, old, new) = match line.as_bytes().first() {
            Some(b'+') => {
                file.added += 1;
                (LineKind::Added, None, Some(self.next_new()))
            }
            Some(b'-') => {
                file.removed += 1;
                (LineKind::Removed, Some(self.next_old()), None)
            }
            // A space, or nothing at all with `diff.suppressBlankEmpty`.
            _ => (
                LineKind::Context,
                Some(self.next_old()),
                Some(self.next_new()),
            ),
        };
        if let Some(hunk) = file.hunks.last_mut() {
            hunk.lines.push(DiffLine {
                kind,
                old,
                new,
                text: line.get(1..).unwrap_or_default().to_owned(),
            });
        }
    }

    fn next_old(&mut self) -> u32 {
        self.old_left = self.old_left.saturating_sub(1);
        self.old += 1;
        self.old - 1
    }

    fn next_new(&mut self) -> u32 {
        self.new_left = self.new_left.saturating_sub(1);
        self.new += 1;
        self.new - 1
    }
}

/// `start,count`, or `start` alone for a count of one.
fn range(text: &str) -> Option<(u32, u32)> {
    match text.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        None => Some((text.parse().ok()?, 1)),
    }
}

#[cfg(test)]
mod tests;
