//! Shows an untracked file as a diff: its whole content as added lines.

use std::fs::{self, Metadata};
use std::io::{self, ErrorKind};
use std::path::Path;

use leon_domain::{DiffLine, FileDiff, FileStatus, Hunk, LineKind};

/// Bigger untracked files are shown as binary, without reading them.
const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;

/// The diff of the untracked file `path` (relative to `root`). `None` when it
/// disappeared since `git status` listed it.
///
/// A symbolic link shows its target, like git does; a nested repository
/// (`path` ends with `/`) shows no content.
pub(crate) fn untracked_diff(root: &Path, path: &str) -> io::Result<Option<FileDiff>> {
    let full = root.join(path);
    let content = match fs::symlink_metadata(&full).and_then(|meta| read(&full, &meta)) {
        Ok(content) => content,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut file = FileDiff {
        path: path.to_owned(),
        status: FileStatus::Untracked,
        added: 0,
        removed: 0,
        binary: false,
        hunks: Vec::new(),
    };
    match content {
        Content::Binary => file.binary = true,
        Content::Empty => {}
        Content::Text(text) => {
            if let Some(hunk) = added_hunk(&text) {
                file.added = u32::try_from(hunk.lines.len()).unwrap_or(u32::MAX);
                file.hunks.push(hunk);
            }
        }
    }
    Ok(Some(file))
}

enum Content {
    Binary,
    /// A directory or another non-regular entry.
    Empty,
    Text(String),
}

fn read(full: &Path, meta: &Metadata) -> io::Result<Content> {
    let bytes = if meta.is_symlink() {
        fs::read_link(full)?.into_os_string().into_encoded_bytes()
    } else if !meta.is_file() {
        return Ok(Content::Empty);
    } else if meta.len() > MAX_TEXT_BYTES {
        return Ok(Content::Binary);
    } else {
        fs::read(full)?
    };
    if bytes.contains(&0) {
        Ok(Content::Binary)
    } else {
        Ok(Content::Text(String::from_utf8_lossy(&bytes).into_owned()))
    }
}

/// One hunk adding every line of `text`, numbered from 1; `None` when empty.
fn added_hunk(text: &str) -> Option<Hunk> {
    let lines: Vec<DiffLine> = text
        .split_terminator('\n')
        .zip(1..)
        .map(|(line, number)| DiffLine {
            kind: LineKind::Added,
            old: None,
            new: Some(number),
            text: line.to_owned(),
        })
        .collect();
    let header = match lines.len() {
        0 => return None,
        1 => "@@ -0,0 +1 @@".to_owned(),
        count => format!("@@ -0,0 +1,{count} @@"),
    };
    Some(Hunk { header, lines })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use leon_domain::FileStatus;

    use super::{MAX_TEXT_BYTES, added_hunk, untracked_diff};

    #[test]
    fn hunk_numbers_every_line_and_ignores_the_final_newline() {
        let cases = [
            ("", None),
            ("one", Some(("@@ -0,0 +1 @@", vec!["one"]))),
            ("a\nb\n", Some(("@@ -0,0 +1,2 @@", vec!["a", "b"]))),
            ("a\n\nc", Some(("@@ -0,0 +1,3 @@", vec!["a", "", "c"]))),
        ];
        for (text, expected) in cases {
            let hunk = added_hunk(text);
            let got = hunk.as_ref().map(|h| {
                let texts: Vec<&str> = h.lines.iter().map(|l| l.text.as_str()).collect();
                (h.header.as_str(), texts)
            });
            assert_eq!(got, expected, "{text:?}");
            let numbers: Vec<_> = hunk.iter().flat_map(|h| &h.lines).map(|l| l.new).collect();
            let expected: Vec<_> = (1..=u32::try_from(numbers.len()).unwrap())
                .map(Some)
                .collect();
            assert_eq!(numbers, expected);
        }
    }

    #[test]
    fn reads_text_binary_links_folders_and_vanished_files() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path();
        fs::write(dir.join("text.txt"), "x\ny\n").unwrap();
        fs::write(dir.join("nul.dat"), b"a\0b").unwrap();
        let big = vec![b'a'; usize::try_from(MAX_TEXT_BYTES).unwrap() + 1];
        fs::write(dir.join("big.txt"), big).unwrap();
        fs::create_dir(dir.join("nested")).unwrap();
        std::os::unix::fs::symlink("nested", dir.join("link")).unwrap();

        let read = |path: &str| untracked_diff(dir, path).unwrap();
        let shape = |path: &str| read(path).map(|f| (f.status, f.added, f.binary, f.hunks.len()));
        assert_eq!(
            shape("text.txt"),
            Some((FileStatus::Untracked, 2, false, 1))
        );
        assert_eq!(shape("nul.dat"), Some((FileStatus::Untracked, 0, true, 0)));
        assert_eq!(shape("big.txt"), Some((FileStatus::Untracked, 0, true, 0)));
        assert_eq!(shape("nested/"), Some((FileStatus::Untracked, 0, false, 0)));
        assert_eq!(read("link").unwrap().hunks[0].lines[0].text, "nested");
        assert_eq!(shape("gone.txt"), None);
    }
}
