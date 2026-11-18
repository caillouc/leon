//! The process loop shared by the CLIs Leon drives over JSON-RPC, one message
//! per line on stdio (Codex app-server, ACP): write what the protocol state
//! machine asks, feed it the CLI's lines and Leon's commands.

use std::ffi::OsString;
use std::path::Path;

use leon_core::ports::{BackendChannel, BackendCommand, BackendEvent, PortError};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::unbounded_channel;

use crate::process::{spawn, stderr_tail};

/// A protocol state machine, free of I/O.
pub(crate) trait Protocol: Send + 'static {
    /// A message from the CLI: events for Leon, messages to send back.
    fn handle(&mut self, msg: &Value) -> (Vec<BackendEvent>, Vec<Value>);
    /// A command from Leon: messages to send.
    fn command(&mut self, command: BackendCommand) -> Vec<Value>;
}

/// How to start the CLI.
pub(crate) struct Launch<'a> {
    pub path: Option<&'a OsString>,
    pub program: &'a str,
    pub args: &'a [String],
    /// Environment variables added for it.
    pub env: &'a [(String, String)],
    pub cwd: &'a Path,
    /// The CLI's name in messages, e.g. "Codex".
    pub label: &'static str,
}

/// Starts the CLI and runs `protocol` on it, `first` being the messages to
/// send at once. A CLI that stops on its own reports its last stderr lines.
pub(crate) fn start(
    launch: &Launch<'_>,
    mut protocol: impl Protocol,
    first: Vec<Value>,
) -> Result<BackendChannel, PortError> {
    let label = launch.label;
    let failed = |cause: &dyn std::fmt::Display| {
        PortError::new("starting a CLI", format!("{label}: {cause}"))
    };
    let mut child = spawn(
        launch.path,
        launch.program,
        launch.args,
        launch.env,
        launch.cwd,
    )
    .map_err(|e| failed(&e))?;
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(failed(&"no stdio pipes"));
    };
    let (commands, mut command_rx) = unbounded_channel();
    let (events, event_rx) = unbounded_channel();
    tokio::spawn(async move {
        let tail = stderr_tail(stderr);
        let mut lines = BufReader::new(stdout).lines();
        let mut exited = false;
        let mut outgoing = first;
        loop {
            for msg in outgoing.drain(..) {
                let line = format!("{msg}\n");
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                    exited = true;
                }
            }
            if exited {
                break;
            }
            tokio::select! {
                line = lines.next_line() => if let Ok(Some(line)) = line {
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                    let (new_events, replies) = protocol.handle(&msg);
                    outgoing = replies;
                    if new_events.into_iter().any(|e| events.send(e).is_err()) {
                        break;
                    }
                } else {
                    exited = true;
                    break;
                },
                command = command_rx.recv() => match command {
                    Some(command) => outgoing = protocol.command(command),
                    None => break,
                },
            }
        }
        let _ = child.start_kill();
        let _ = child.wait().await;
        let tail = tail.await.unwrap_or_default();
        if exited {
            let message = if tail.is_empty() {
                format!("{label} stopped")
            } else {
                tail
            };
            let _ = events.send(BackendEvent::Failed(message));
        }
    });
    Ok(BackendChannel {
        commands,
        events: event_rx,
    })
}
