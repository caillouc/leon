use leon_domain::{DiffLine, FileDiff, FileStatus, LineKind};

use super::{parse_diff, unquote};

/// `(path, status, added, removed, binary, hunk count)` of each file.
fn summary(files: &[FileDiff]) -> Vec<(&str, FileStatus, u32, u32, bool, usize)> {
    files
        .iter()
        .map(|f| {
            let (path, status) = (f.path.as_str(), f.status);
            (path, status, f.added, f.removed, f.binary, f.hunks.len())
        })
        .collect()
}

fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
    DiffLine {
        kind,
        old,
        new,
        text: text.to_owned(),
    }
}

const MODIFIED: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,3 +2,4 @@ fn main() {
 keep
-old
+new
+more
 tail
@@ -10 +11 @@
-x
+y
";

#[test]
fn modified_file_has_numbered_lines_in_each_hunk() {
    let files = parse_diff(MODIFIED);
    assert_eq!(
        summary(&files),
        [("src/lib.rs", FileStatus::Modified, 3, 2, false, 2)]
    );
    let first = &files[0].hunks[0];
    assert_eq!(first.header, "@@ -2,3 +2,4 @@ fn main() {");
    assert_eq!(
        first.lines,
        [
            line(LineKind::Context, Some(2), Some(2), "keep"),
            line(LineKind::Removed, Some(3), None, "old"),
            line(LineKind::Added, None, Some(3), "new"),
            line(LineKind::Added, None, Some(4), "more"),
            line(LineKind::Context, Some(4), Some(5), "tail"),
        ]
    );
    assert_eq!(
        files[0].hunks[1].lines,
        [
            line(LineKind::Removed, Some(10), None, "x"),
            line(LineKind::Added, None, Some(11), "y"),
        ]
    );
}

const EVERY_KIND: &str = "\
diff --git a/added.txt b/added.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/added.txt
@@ -0,0 +1,2 @@
+one
+two
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 4444444..0000000
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/image.png b/image.png
index 5555555..6666666 100644
Binary files a/image.png and b/image.png differ
diff --git a/old name.txt b/new name.txt
similarity index 80%
rename from old name.txt
rename to new name.txt
index 7777777..8888888 100644
--- a/old name.txt\t
+++ b/new name.txt\t
@@ -1,2 +1,2 @@
 same
-before
+after
diff --git a/moved.txt b/dir/moved.txt
similarity index 100%
rename from moved.txt
rename to dir/moved.txt
diff --git a/run.sh b/run.sh
old mode 100644
new mode 100755
diff --git a/empty.txt b/empty.txt
new file mode 100644
index 0000000..e69de29
";

#[test]
fn reads_status_path_and_binary_of_every_kind_of_change() {
    assert_eq!(
        summary(&parse_diff(EVERY_KIND)),
        [
            ("added.txt", FileStatus::Added, 2, 0, false, 1),
            ("gone.txt", FileStatus::Deleted, 0, 1, false, 1),
            ("image.png", FileStatus::Modified, 0, 0, true, 0),
            ("new name.txt", FileStatus::Renamed, 1, 1, false, 1),
            ("dir/moved.txt", FileStatus::Renamed, 0, 0, false, 0),
            ("run.sh", FileStatus::Modified, 0, 0, false, 0),
            ("empty.txt", FileStatus::Added, 0, 0, false, 0),
        ]
    );
}

/// Content lines that look like headers, a missing final newline, and an
/// empty context line (`diff.suppressBlankEmpty`).
const TRICKY_CONTENT: &str = "\
diff --git a/notes.md b/notes.md
index 1111111..2222222 100644
--- a/notes.md
+++ b/notes.md
@@ -1,4 +1,4 @@
--- a/heading
+++ b/heading

 diff --git a/x b/x
-last
\\ No newline at end of file
+last!
\\ No newline at end of file
";

#[test]
fn content_lines_are_never_taken_for_headers() {
    let files = parse_diff(TRICKY_CONTENT);
    assert_eq!(
        summary(&files),
        [("notes.md", FileStatus::Modified, 2, 2, false, 1)]
    );
    assert_eq!(
        files[0].hunks[0].lines,
        [
            line(LineKind::Removed, Some(1), None, "-- a/heading"),
            line(LineKind::Added, None, Some(1), "++ b/heading"),
            line(LineKind::Context, Some(2), Some(2), ""),
            line(LineKind::Context, Some(3), Some(3), "diff --git a/x b/x"),
            line(LineKind::Removed, Some(4), None, "last"),
            line(LineKind::Added, None, Some(4), "last!"),
        ]
    );
}

#[test]
fn quoted_paths_are_unquoted() {
    let diff = "\
diff --git \"a/tab\\there.txt\" \"b/tab\\there.txt\"
new file mode 100644
--- /dev/null
+++ \"b/tab\\there.txt\"
@@ -0,0 +1 @@
+x
diff --git \"a/caf\\303\\251.txt\" \"b/new\\\"name.txt\"
similarity index 100%
rename from \"caf\\303\\251.txt\"
rename to \"new\\\"name.txt\"
";
    let paths: Vec<_> = parse_diff(diff).into_iter().map(|f| f.path).collect();
    assert_eq!(paths, ["tab\there.txt", "new\"name.txt"]);
}

#[test]
fn unquote_handles_escapes_and_plain_text() {
    let cases = [
        ("plain/path.txt", "plain/path.txt"),
        ("\"a\\tb\"", "a\tb"),
        ("\"caf\\303\\251\"", "café"),
        ("\"q\\\"\\\\\"", "q\"\\"),
        ("\"line\\nbreak\"", "line\nbreak"),
        ("\"\\001\"", "\u{1}"),
        ("\"unknown\\q\"", "unknown\\q"),
    ];
    for (quoted, expected) in cases {
        assert_eq!(unquote(quoted), expected, "{quoted}");
    }
}

#[test]
fn empty_output_has_no_files() {
    assert_eq!(parse_diff(""), []);
}
