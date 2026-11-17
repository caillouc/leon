//! Reads `git status --porcelain=v2 -z --branch` output.

/// What `changes` needs from `git status`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Status {
    /// The branch has no commit yet: there is no `HEAD` to compare with.
    pub(crate) unborn: bool,
    /// Untracked paths, relative to the repository root. A nested repository
    /// ends with `/`.
    pub(crate) untracked: Vec<String>,
}

/// Parses NUL-separated porcelain v2 records.
pub(crate) fn parse(output: &[u8]) -> Status {
    let mut status = Status::default();
    let mut fields = output.split(|byte| *byte == 0);
    while let Some(field) = fields.next() {
        if field == b"# branch.oid (initial)" {
            status.unborn = true;
        } else if let Some(path) = field.strip_prefix(b"? ") {
            status
                .untracked
                .push(String::from_utf8_lossy(path).into_owned());
        } else if field.starts_with(b"2 ") {
            // A rename or copy: its original path is the next field.
            fields.next();
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::{Status, parse};

    #[test]
    fn reads_unborn_branch_and_untracked_paths() {
        let renamed = "2 R. N... 100644 100644 100644 aaa aaa R100 new.txt";
        let cases: [(String, Status); 4] = [
            (String::new(), Status::default()),
            (
                "# branch.oid (initial)\0# branch.head main\0? a.txt\0".to_owned(),
                Status {
                    unborn: true,
                    untracked: vec!["a.txt".to_owned()],
                },
            ),
            (
                "# branch.oid 1234\0? dir/with space.txt\0? vendor/lib/\0".to_owned(),
                Status {
                    unborn: false,
                    untracked: vec!["dir/with space.txt".to_owned(), "vendor/lib/".to_owned()],
                },
            ),
            (
                // The original path of a rename is not an untracked file,
                // even when it looks like one.
                format!("{renamed}\0? old.txt\0? b.txt\0"),
                Status {
                    unborn: false,
                    untracked: vec!["b.txt".to_owned()],
                },
            ),
        ];
        for (output, expected) in cases {
            assert_eq!(parse(output.as_bytes()), expected, "{output:?}");
        }
    }
}
