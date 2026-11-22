//! The process loop shared by the CLIs Leon drives over JSON-RPC, one message
//! per line on stdio (Codex app-server, ACP): write what the protocol state
//! machine asks, feed it the CLI's lines and Leon's commands.

use leon_core::ports::{BackendChannel, BackendCommand, BackendEvent};
use serde_json::Value;
use tokio::sync::mpsc::unbounded_channel;

use crate::conn::{Opening, ended};

/// A protocol state machine, free of I/O.
pub(crate) trait Protocol: Send + 'static {
    /// A message from the CLI: events for Leon, messages to send back.
    fn handle(&mut self, msg: &Value) -> (Vec<BackendEvent>, Vec<Value>);
    /// A command from Leon: messages to send.
    fn command(&mut self, command: BackendCommand) -> Vec<Value>;
}

/// Runs `protocol` on the CLI `opening` started or found again, `first`
/// being the messages to send at once; `label` names the CLI in messages,
/// e.g. "Codex". A CLI that stops on its own reports its last stderr lines;
/// one Leon lets go of is stopped.
pub(crate) fn session(
    opening: Opening,
    label: &'static str,
    mut protocol: impl Protocol,
    first: Vec<Value>,
) -> BackendChannel {
    let (commands, mut command_rx) = unbounded_channel();
    let (events, event_rx) = unbounded_channel();
    tokio::spawn(async move {
        let (mut input, mut output) = match opening.connect().await {
            Ok(conn) => conn,
            Err(error) => {
                let _ = events.send(BackendEvent::Failed(format!("{label}: {error}")));
                return;
            }
        };
        let mut exited = false;
        let mut outgoing = first;
        loop {
            for msg in outgoing.drain(..) {
                if !input.send(&msg.to_string()).await {
                    exited = true;
                }
            }
            if exited {
                break;
            }
            tokio::select! {
                line = output.next_line() => if let Some(line) = line {
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
        if !exited {
            input.stop().await;
            return;
        }
        let (ok, tail) = ended(input, output).await;
        if !ok {
            let message = if tail.is_empty() {
                format!("{label} stopped")
            } else {
                tail
            };
            let _ = events.send(BackendEvent::Failed(message));
        }
    });
    BackendChannel {
        commands,
        events: event_rx,
    }
}
