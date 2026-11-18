//! Starting CLI processes the way a terminal would.

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;

use leon_domain::BackendKind;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};
use tokio::task::JoinHandle;

/// The program that runs `backend`'s CLI.
pub(crate) fn program(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::ClaudeCode => "claude",
        BackendKind::Codex => "codex",
        BackendKind::CopilotCli => "copilot",
    }
}

/// `items` as owned arguments for [`run`].
pub(crate) fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

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

/// Starts `program` in `cwd` with piped stdio and `env` added (see
/// [`command`]). The process is killed when the returned [`Child`] is
/// dropped.
pub(crate) fn spawn(
    path: Option<&OsString>,
    program: &str,
    args: &[String],
    env: &[(String, String)],
    cwd: &Path,
) -> std::io::Result<Child> {
    let mut cmd = Command::from(command(path, program, args, cwd));
    cmd.envs(env.iter().map(|(k, v)| (k, v)));
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd.spawn()
}

/// Why a one-off command gave no output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunError {
    /// The program is not installed (not on `PATH`).
    NotFound,
    /// It took longer than allowed and was stopped.
    TimedOut,
    /// Anything else, as the system said it.
    Other(String),
}

/// What a one-off command printed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Output {
    /// It exited with success.
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs a one-off command in the temp folder with no input, and stops it
/// after `timeout`. Both outputs are read while it runs, so a chatty CLI
/// cannot fill a pipe and stall.
pub(crate) fn run(
    path: Option<&OsString>,
    program: &str,
    args: &[String],
    timeout: std::time::Duration,
) -> Result<Output, RunError> {
    use std::io::Read;
    let mut child = command(path, program, args, &std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => RunError::NotFound,
            _ => RunError::Other(e.to_string()),
        })?;
    let reader = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        })
    };
    let stdout = reader(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = reader(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut);
            }
            Err(e) => return Err(RunError::Other(e.to_string())),
        }
    };
    Ok(Output {
        ok: status.success(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
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
