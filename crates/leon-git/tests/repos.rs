//! `GitCli` against temporary repositories created by each test.

#![allow(clippy::unwrap_used, reason = "test helpers fail the test on purpose")]

use std::fs;
use std::path::Path;
use std::process::Command;

use leon_core::ports::Git;
use leon_domain::{FileDiff, FileStatus, LineKind};
use leon_git::GitCli;
use tempfile::TempDir;

/// Runs a setup command in a test repository, isolated from the user's git
/// configuration (no hooks, no signing) and from any inherited repository;
/// its output, trimmed.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "init.defaultBranch=main"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    dir
}

fn write(dir: &Path, path: &str, content: impl AsRef<[u8]>) {
    let path = dir.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn commit_all(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// `(path, status, added, removed, binary)` of each file.
fn summary(files: &[FileDiff]) -> Vec<(&str, FileStatus, u32, u32, bool)> {
    files
        .iter()
        .map(|f| (f.path.as_str(), f.status, f.added, f.removed, f.binary))
        .collect()
}

#[test]
fn outside_a_repository_every_call_fails_or_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let error = GitCli::default().changes(dir.path()).unwrap_err();
    assert_eq!(error.context, "reading git changes");
    assert!(error.message.contains("not a git repository"), "{error}");
    assert!(GitCli::default().branch_diff(dir.path(), "main").is_err());
    assert_eq!(GitCli::default().branch(dir.path()), None);
    assert_eq!(GitCli::default().default_base(dir.path()), None);
}

#[test]
fn repository_without_commits_shows_staged_and_untracked_files() {
    let repo = repo();
    let dir = repo.path();
    write(dir, "staged.txt", "s\n");
    git(dir, &["add", "staged.txt"]);
    write(dir, "notes.txt", "one\ntwo\n");

    let files = GitCli::default().changes(dir).unwrap();
    assert_eq!(
        summary(&files),
        [
            ("notes.txt", FileStatus::Untracked, 2, 0, false),
            ("staged.txt", FileStatus::Added, 1, 0, false),
        ]
    );
    let numbers: Vec<_> = files[0].hunks[0].lines.iter().map(|l| l.new).collect();
    assert_eq!(numbers, [Some(1), Some(2)]);
    assert_eq!(GitCli::default().branch(dir), None);
    assert_eq!(GitCli::default().default_base(dir), None);
}

#[test]
fn changes_cover_every_kind_of_file_whatever_the_user_config() {
    let repo = repo();
    let dir = repo.path();
    write(dir, ".gitignore", "*.log\n");
    write(dir, "keep.txt", "a\nb\nc\n");
    write(dir, "gone.txt", "bye\n");
    write(dir, "old.txt", "1\n2\n3\n4\n5\n");
    write(dir, "image.bin", b"\x00\x01\x02");
    commit_all(dir, "init");
    // Settings that would change git's output if the adapter did not
    // override them.
    for (key, value) in [("diff.noprefix", "true"), ("color.ui", "always")] {
        git(dir, &["config", key, value]);
    }

    write(dir, "keep.txt", "a\nB\nc\n");
    git(dir, &["add", "keep.txt"]);
    write(dir, "keep.txt", "a\nB\nc\nd\n");
    fs::remove_file(dir.join("gone.txt")).unwrap();
    git(dir, &["mv", "old.txt", "new.txt"]);
    write(dir, "new.txt", "1\n2\n3\n4\nfive\n");
    write(dir, "image.bin", b"\x00\x09");
    write(dir, "docs/untracked.md", "# Title\n");
    write(dir, "blob.dat", b"x\x00y");
    write(dir, "debug.log", "ignored\n");
    git(&dir.join("docs"), &["init", "-q", "nested"]);

    let files = GitCli::default().changes(dir).unwrap();
    assert_eq!(
        summary(&files),
        [
            ("blob.dat", FileStatus::Untracked, 0, 0, true),
            ("docs/nested/", FileStatus::Untracked, 0, 0, false),
            ("docs/untracked.md", FileStatus::Untracked, 1, 0, false),
            ("gone.txt", FileStatus::Deleted, 0, 1, false),
            ("image.bin", FileStatus::Modified, 0, 0, true),
            ("keep.txt", FileStatus::Modified, 2, 1, false),
            ("new.txt", FileStatus::Renamed, 1, 1, false),
        ]
    );
    let keep = &files[5].hunks[0].lines;
    let changed: Vec<_> = keep
        .iter()
        .filter(|l| l.kind != LineKind::Context)
        .map(|l| (l.kind, l.old, l.new, l.text.as_str()))
        .collect();
    assert_eq!(
        changed,
        [
            (LineKind::Removed, Some(2), None, "b"),
            (LineKind::Added, None, Some(2), "B"),
            (LineKind::Added, None, Some(4), "d"),
        ]
    );
}

#[test]
fn changes_from_a_subfolder_use_paths_from_the_root() {
    let repo = repo();
    let dir = repo.path();
    write(dir, "src/main.rs", "fn main() {}\n");
    commit_all(dir, "init");
    write(dir, "src/main.rs", "fn main() {}\n// end\n");
    write(dir, "other/new.txt", "hello\n");

    let files = GitCli::default().changes(&dir.join("src")).unwrap();
    assert_eq!(
        summary(&files),
        [
            ("other/new.txt", FileStatus::Untracked, 1, 0, false),
            ("src/main.rs", FileStatus::Modified, 1, 0, false),
        ]
    );
    assert_eq!(files[0].hunks[0].lines[0].text, "hello");
}

#[test]
fn branch_diff_shows_only_commits_since_the_merge_base() {
    let repo = repo();
    let dir = repo.path();
    write(dir, "shared.txt", "base\n");
    commit_all(dir, "base");
    git(dir, &["checkout", "-q", "-b", "feature"]);
    write(dir, "feature.txt", "mine\n");
    commit_all(dir, "feature work");
    git(dir, &["checkout", "-q", "main"]);
    write(dir, "shared.txt", "base\nmain moved on\n");
    commit_all(dir, "main work");
    git(dir, &["checkout", "-q", "feature"]);
    write(dir, "feature.txt", "mine\nnot committed\n");

    assert_eq!(GitCli::default().branch(dir), Some("feature".to_owned()));
    let files = GitCli::default().branch_diff(dir, "main").unwrap();
    assert_eq!(
        summary(&files),
        [("feature.txt", FileStatus::Added, 1, 0, false)]
    );

    let error = GitCli::default()
        .branch_diff(dir, "no-such-branch")
        .unwrap_err();
    assert_eq!(error.context, "reading the branch diff");
    let error = GitCli::default()
        .branch_diff(dir, "--output=leak.txt")
        .unwrap_err();
    assert!(error.message.contains("--output=leak.txt"), "{error}");
    assert!(!dir.join("leak.txt").exists());
}

#[test]
fn detached_head_has_no_branch() {
    let repo = repo();
    let dir = repo.path();
    write(dir, "a.txt", "a\n");
    commit_all(dir, "init");
    assert_eq!(GitCli::default().branch(dir), Some("main".to_owned()));
    git(dir, &["checkout", "-q", "--detach"]);
    assert_eq!(GitCli::default().branch(dir), None);
}

#[test]
fn default_base_prefers_the_remote_default_branch() {
    let repo = repo();
    let dir = repo.path();
    write(dir, "a.txt", "a\n");
    commit_all(dir, "init");
    assert_eq!(GitCli::default().default_base(dir), Some("main".to_owned()));

    git(dir, &["update-ref", "refs/remotes/origin/master", "HEAD"]);
    assert_eq!(
        GitCli::default().default_base(dir),
        Some("origin/master".to_owned())
    );

    git(dir, &["update-ref", "refs/remotes/origin/trunk", "HEAD"]);
    let head = [
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/trunk",
    ];
    git(dir, &head);
    assert_eq!(
        GitCli::default().default_base(dir),
        Some("origin/trunk".to_owned())
    );
}

#[test]
fn a_file_belongs_to_the_working_tree_holding_it() {
    let dir = repo();
    write(dir.path(), "src/a.rs", "a\n");
    commit_all(dir.path(), "first");
    git(
        dir.path(),
        &["worktree", "add", "-q", "wt/fix", "-b", "fix"],
    );
    let root = dir.path().canonicalize().unwrap();
    let git = GitCli::default();
    let cases = [
        // file, working tree
        ("src/a.rs", root.clone()),
        ("src/gone/b.rs", root.clone()),
        ("wt/fix/src/a.rs", root.join("wt/fix")),
        ("wt/fix", root.join("wt/fix")),
    ];
    for (file, expected) in cases {
        assert_eq!(
            git.working_tree(&dir.path().join(file)),
            Some(expected),
            "{file}"
        );
    }
    let outside = tempfile::tempdir().unwrap();
    assert_eq!(git.working_tree(&outside.path().join("a.rs")), None);
}

/// A fetch brings `origin/<branch>` up to date and nothing else: files, the
/// current branch and `FETCH_HEAD` stay as they were (I22).
#[test]
fn a_fetch_moves_only_the_remote_tracking_branch() {
    let remote = repo();
    write(remote.path(), "a.rs", "a\n");
    commit_all(remote.path(), "first");
    let clone = tempfile::tempdir().unwrap();
    git(
        clone.path(),
        &["clone", "-q", remote.path().to_str().unwrap(), "."],
    );
    let head = git(clone.path(), &["rev-parse", "HEAD"]);
    write(remote.path(), "b.rs", "b\n");
    commit_all(remote.path(), "second");

    GitCli::default().fetch(clone.path(), "main").unwrap();

    let ahead = git(remote.path(), &["rev-parse", "HEAD"]);
    assert_eq!(git(clone.path(), &["rev-parse", "origin/main"]), ahead);
    assert_eq!(git(clone.path(), &["rev-parse", "HEAD"]), head);
    assert!(!clone.path().join("b.rs").exists());
    assert!(!clone.path().join(".git/FETCH_HEAD").exists());
    let error = GitCli::default().fetch(clone.path(), "gone").unwrap_err();
    assert_eq!(error.context, "fetching the base branch");
}
