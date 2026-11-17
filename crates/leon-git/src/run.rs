//! Runs the `git` command and returns its output.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

/// Options set on every call: paths printed as they are (not octal-escaped),
/// no index refresh written to the repository (D4).
const GLOBAL_ARGS: [&str; 3] = ["--no-optional-locks", "-c", "core.quotepath=off"];

/// Inherited variables that would make git read another repository than the
/// one at `dir`.
const REPO_ENV: [&str; 5] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
];

/// Runs `git <args>` in `dir`, with `path` as `PATH` if given: its standard output on success, git's own
/// message on failure.
pub(crate) fn git(path: Option<&OsString>, dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(GLOBAL_ARGS)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    for name in REPO_ENV {
        command.env_remove(name);
    }
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let output = command
        .output()
        .map_err(|e| format!("could not start git: {e}"))?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if message.is_empty() {
        let command = args.first().copied().unwrap_or_default();
        Err(format!("git {command} failed ({})", output.status))
    } else {
        Err(message)
    }
}

/// Runs `git <args>` in `dir` and returns its output as one trimmed line.
pub(crate) fn git_line(
    path: Option<&OsString>,
    dir: &Path,
    args: &[&str],
) -> Result<String, String> {
    let output = git(path, dir, args)?;
    Ok(String::from_utf8_lossy(&output).trim().to_owned())
}
