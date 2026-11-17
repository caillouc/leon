//! Reads a working directory's git state by running the `git` command with
//! its machine-readable output (P6): current branch, default base,
//! uncommitted changes and branch diff (GIT-1..3).
//!
//! Must not: change a repository in any way (D4) — only read-only commands,
//! with `--no-optional-locks` so not even the index is refreshed; prompt for
//! credentials; or depend on another adapter.

mod parse;
mod run;
mod status;
mod untracked;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use leon_core::ports::{Git, PortError, SearchPath, inherited_path};
use leon_domain::FileDiff;

use crate::parse::parse_diff;
use crate::run::{git, git_line};
use crate::untracked::untracked_diff;

/// The [`Git`] port, backed by the `git` command found on `PATH`.
#[derive(Debug, Clone)]
pub struct GitCli {
    path: SearchPath,
}

impl GitCli {
    /// Runs `git` with the `PATH` that `path` gives.
    pub fn new(path: SearchPath) -> Self {
        Self { path }
    }
}

impl Default for GitCli {
    /// Runs `git` with the inherited `PATH`.
    fn default() -> Self {
        Self::new(inherited_path)
    }
}

/// Bases tried, in order, when the remote has no default branch recorded.
const FALLBACK_BASES: [&str; 4] = ["origin/main", "origin/master", "main", "master"];

/// `git diff` options that make the output parseable whatever the user's
/// configuration: no colour, no external tool, standard prefixes, paths from
/// the repository root, renames detected.
const DIFF_ARGS: [&str; 8] = [
    "diff",
    "--no-color",
    "--no-ext-diff",
    "--no-relative",
    "--submodule=short",
    "-M",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

impl Git for GitCli {
    fn doctor(&self) -> leon_domain::DoctorSection {
        use leon_domain::{DoctorCheck, Health};
        let check = match git_line((self.path)(), &std::env::temp_dir(), &["--version"]) {
            Ok(version) => DoctorCheck::new(Health::Ok, "Installed", version),
            Err(error) => DoctorCheck::new(Health::Problem, "Installed", error)
                .fix("Install git so that `git` is on your PATH: Leon reads changes with it"),
        };
        leon_domain::DoctorSection {
            title: "git".to_owned(),
            checks: vec![check],
        }
    }

    fn branch(&self, dir: &Path) -> Option<String> {
        let name = git_line((self.path)(), dir, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
        (!name.is_empty() && name != "HEAD").then_some(name)
    }

    fn default_base(&self, dir: &Path) -> Option<String> {
        let path = (self.path)();
        let remote_head = ["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"];
        if let Ok(base) = git_line(path, dir, &remote_head)
            && !base.is_empty()
        {
            return Some(base);
        }
        FALLBACK_BASES
            .into_iter()
            .find(|base| git(path, dir, &["rev-parse", "--verify", "--quiet", base]).is_ok())
            .map(str::to_owned)
    }

    fn changes(&self, dir: &Path) -> Result<Vec<FileDiff>, PortError> {
        changes((self.path)(), dir).map_err(|e| PortError::new("reading git changes", e))
    }

    fn branch_diff(&self, dir: &Path, base: &str) -> Result<Vec<FileDiff>, PortError> {
        branch_diff((self.path)(), dir, base)
            .map_err(|e| PortError::new("reading the branch diff", e))
    }
}

/// Working tree against `HEAD` (staged and unstaged), plus untracked files
/// that `.gitignore` does not exclude, sorted by path.
fn changes(path: Option<&OsString>, dir: &Path) -> Result<Vec<FileDiff>, String> {
    let status = git(
        path,
        dir,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
        ],
    )?;
    let status = status::parse(&status);
    let base = if status.unborn {
        // No commit yet: everything tracked is new.
        git_line(path, dir, &["hash-object", "-t", "tree", "--stdin"])?
    } else {
        "HEAD".to_owned()
    };
    let mut files = diff(path, dir, &[&base])?;
    if !status.untracked.is_empty() {
        let root = PathBuf::from(git_line(path, dir, &["rev-parse", "--show-toplevel"])?);
        for path in &status.untracked {
            if let Some(file) = untracked_diff(&root, path).map_err(|e| format!("{path}: {e}"))? {
                files.push(file);
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Committed changes since the branch left `base`: `merge-base(base, HEAD)..HEAD`.
fn branch_diff(path: Option<&OsString>, dir: &Path, base: &str) -> Result<Vec<FileDiff>, String> {
    let merge_base = git_line(path, dir, &["merge-base", "--end-of-options", base, "HEAD"])
        .map_err(|e| format!("finding the merge base of {base} and HEAD: {e}"))?;
    let mut files = diff(path, dir, &[&merge_base, "HEAD"])?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Parsed `git diff <revs>`.
fn diff(path: Option<&OsString>, dir: &Path, revs: &[&str]) -> Result<Vec<FileDiff>, String> {
    let mut args = DIFF_ARGS.to_vec();
    args.extend_from_slice(revs);
    args.push("--");
    let output = git(path, dir, &args)?;
    Ok(parse_diff(&String::from_utf8_lossy(&output)))
}
