//! Runs `gh` (never through a shell) and sorts its failures.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// `gh`'s exit code when a command needs a login (`gh help exit-codes`,
/// 2.101.0).
const AUTH_REQUIRED: i32 = 4;

/// What `gh` 2.101.0 prints when the folder has no GitHub repository to fill
/// `{owner}` and `{repo}` with (recorded 2026-11-17).
const NO_GITHUB_REPO: &[&str] = &[
    "not a git repository",
    "no git remotes found",
    "point to a known GitHub host",
];

/// Why a `gh` run failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GhError {
    /// `gh` is not installed or not on `PATH`.
    Missing,
    /// `gh` has no GitHub login.
    LoggedOut,
    /// The folder is not a git repository with a GitHub remote.
    NoGitHubRepo,
    /// Anything else, with what `gh` said.
    Failed(String),
}

impl fmt::Display for GhError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str(
                "the GitHub CLI `gh` was not found on PATH: install it \
                 (https://cli.github.com), then run `gh auth login`",
            ),
            Self::LoggedOut => f.write_str("not logged in to GitHub: run `gh auth login`"),
            Self::NoGitHubRepo => f.write_str("not a git repository with a GitHub remote"),
            Self::Failed(message) => write!(f, "gh failed: {message}"),
        }
    }
}

/// Runs `gh args…`, in `dir` if given, and returns what it printed.
pub(crate) fn gh(
    path: Option<&OsString>,
    args: &[String],
    dir: Option<&Path>,
) -> Result<String, GhError> {
    let out = command(path, args, dir)
        .output()
        .map_err(|e| spawn_error(&e, dir))?;
    if out.status.success() {
        return String::from_utf8(out.stdout).map_err(|e| GhError::Failed(e.to_string()));
    }
    Err(classify(
        out.status.code(),
        &String::from_utf8_lossy(&out.stderr),
    ))
}

/// The `gh` command: no prompt, no colour, no update notice, and the
/// repository taken from `dir`, never from `GH_REPO`.
fn command(path: Option<&OsString>, args: &[String], dir: Option<&Path>) -> Command {
    let mut cmd = Command::new("gh");
    cmd.args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env_remove("GH_REPO")
        .stdin(Stdio::null());
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    if let Some(path) = path {
        cmd.env("PATH", path);
    }
    cmd
}

/// `gh` could not be started: a missing folder also reads as "not found".
fn spawn_error(error: &io::Error, dir: Option<&Path>) -> GhError {
    match dir {
        Some(dir) if !dir.is_dir() => GhError::Failed("the folder does not exist".into()),
        _ if error.kind() == io::ErrorKind::NotFound => GhError::Missing,
        _ => GhError::Failed(error.to_string()),
    }
}

/// Sorts a failed run by its exit code and what it printed on stderr.
fn classify(code: Option<i32>, stderr: &str) -> GhError {
    if code == Some(AUTH_REQUIRED) {
        return GhError::LoggedOut;
    }
    if NO_GITHUB_REPO.iter().any(|m| stderr.contains(m)) {
        return GhError::NoGitHubRepo;
    }
    let message = stderr.trim();
    GhError::Failed(if message.is_empty() {
        format!("exit code {code:?}")
    } else {
        message.to_owned()
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::io;
    use std::path::Path;

    use super::{GhError, classify, command, spawn_error};

    /// Recorded from gh 2.101.0 (2026-11-17).
    #[test]
    fn recorded_failures() {
        let cases = [
            (
                1,
                include_str!("../fixtures/gh/2.101.0/stderr-not-a-git-repository.txt"),
                GhError::NoGitHubRepo,
            ),
            (
                1,
                include_str!("../fixtures/gh/2.101.0/stderr-no-remotes.txt"),
                GhError::NoGitHubRepo,
            ),
            (
                1,
                include_str!("../fixtures/gh/2.101.0/stderr-not-a-github-remote.txt"),
                GhError::NoGitHubRepo,
            ),
            (
                4,
                include_str!("../fixtures/gh/2.101.0/stderr-not-logged-in.txt"),
                GhError::LoggedOut,
            ),
        ];
        for (code, stderr, expected) in cases {
            assert_eq!(classify(Some(code), stderr), expected, "{stderr}");
        }
    }

    #[test]
    fn other_failures_keep_what_gh_said() {
        let offline = include_str!("../fixtures/gh/2.101.0/stderr-offline.txt");
        assert!(
            matches!(classify(Some(1), offline), GhError::Failed(m) if m.ends_with("connection refused"))
        );
        assert_eq!(classify(None, ""), GhError::Failed("exit code None".into()));
    }

    #[test]
    fn errors_say_what_to_do() {
        assert!(GhError::Missing.to_string().contains("gh auth login"));
        assert!(GhError::LoggedOut.to_string().contains("gh auth login"));
    }

    #[test]
    fn spawn_errors() {
        let not_found = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(spawn_error(&not_found, None), GhError::Missing);
        assert_eq!(
            spawn_error(&not_found, Some(Path::new("/leon-test-missing-folder"))),
            GhError::Failed("the folder does not exist".into())
        );
    }

    #[test]
    fn command_never_prompts_nor_uses_gh_repo() {
        let path = std::ffi::OsString::from("/login/bin");
        let cmd = command(Some(&path), &["api".into()], Some(Path::new("/x")));
        let envs: Vec<_> = cmd.get_envs().collect();
        for var in ["GH_PROMPT_DISABLED", "NO_COLOR", "GH_NO_UPDATE_NOTIFIER"] {
            assert!(envs.contains(&(OsStr::new(var), Some(OsStr::new("1")))));
        }
        assert!(envs.contains(&(OsStr::new("GH_REPO"), None)));
        assert!(envs.contains(&(OsStr::new("PATH"), Some(OsStr::new("/login/bin")))));
        assert_eq!(cmd.get_program(), "gh");
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/x")));
    }
}
