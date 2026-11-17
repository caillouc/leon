//! Starting CLI processes the way a terminal would.

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};
use tokio::task::JoinHandle;

/// Variables a host coding app sets for its own child sessions. Inherited by
/// a CLI started from Leon, they would make it behave as a child of that app
/// (e.g. skip the user's login), so they are removed.
const HOST_SESSION_VARS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SIMPLE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_HOST_SESSION_ID",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SDK_HAS_HOST_AUTH_REFRESH",
    "CLAUDE_AGENT_SDK_VERSION",
    "CLAUDE_AGENT_SDK_MCP_NO_PREFIX",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];

/// `program` with `args` in `cwd`, as a terminal would start it: `path` as
/// `PATH` if given, without a host app's session variables.
pub(crate) fn command(
    path: Option<&OsString>,
    program: &str,
    args: &[String],
    cwd: &Path,
) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args).current_dir(cwd);
    for var in HOST_SESSION_VARS {
        cmd.env_remove(var);
    }
    if let Some(path) = path {
        cmd.env("PATH", path);
    }
    cmd
}

/// Starts `program` in `cwd` with piped stdio (see [`command`]). The process
/// is killed when the returned [`Child`] is dropped.
pub(crate) fn spawn(
    path: Option<&OsString>,
    program: &str,
    args: &[String],
    cwd: &Path,
) -> std::io::Result<Child> {
    let mut cmd = Command::from(command(path, program, args, cwd));
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Keeps the last lines the CLI wrote on stderr, for error messages.
pub(crate) fn stderr_tail(stderr: ChildStderr) -> JoinHandle<String> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: Vec<String> = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(target: "leon_backends::stderr", "{line}");
            tail.push(line);
            if tail.len() > 12 {
                tail.remove(0);
            }
        }
        tail.join("\n")
    })
}

/// Cuts `text` to at most `max` bytes on a character boundary.
pub(crate) fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::clip;

    #[test]
    fn clip_respects_char_boundaries() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("abcdef", 3), "abc…");
        assert_eq!(clip("ééé", 3), "é…");
    }
}
