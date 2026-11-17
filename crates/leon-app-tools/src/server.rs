//! The app side: listens on the Unix socket, one task per bridge connection.

use std::os::unix::fs::{DirBuilderExt, FileTypeExt};
use std::path::Path;
use std::time::Duration;

use leon_core::ports::{AppToolCall, AppToolRequest, AppToolSender};
use leon_domain::AgentSessionId;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, WriteHalf};
use tokio::net::UnixListener;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use crate::Error;
use crate::protocol::{self, Reply};

/// What the agent reads when the core cannot take its call.
const NOT_RUNNING: &str = "Leon is not running";

/// Listens on `socket` and sends every tool call to `tools`. Returns once
/// listening; connections are served on `runtime`.
///
/// Creates the socket's folder (mode 0700) if missing and replaces a socket
/// file left by an earlier run.
pub fn serve(runtime: &Handle, socket: &Path, tools: AppToolSender) -> Result<(), Error> {
    if let Some(dir) = socket.parent().filter(|dir| !dir.exists()) {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|source| Error::CreateDir {
                dir: dir.to_owned(),
                source,
            })?;
    }
    if std::fs::symlink_metadata(socket).is_ok_and(|meta| meta.file_type().is_socket()) {
        if std::os::unix::net::UnixStream::connect(socket).is_ok() {
            return Err(Error::InUse {
                socket: socket.to_owned(),
            });
        }
        std::fs::remove_file(socket).map_err(|source| Error::RemoveStale {
            socket: socket.to_owned(),
            source,
        })?;
    }
    let listener = {
        let _context = runtime.enter();
        UnixListener::bind(socket).map_err(|source| Error::Bind {
            socket: socket.to_owned(),
            source,
        })?
    };
    tracing::debug!(socket = %socket.display(), "app tools listening");
    runtime.spawn(accept(listener, tools));
    Ok(())
}

async fn accept(listener: UnixListener, tools: AppToolSender) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(connection(stream, tools.clone()));
            }
            Err(error) => {
                tracing::warn!(%error, "app tools: could not accept a connection");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Serves one bridge: its `{"session": <id>}` line, then MCP messages until
/// it closes. Tool calls run concurrently; answers go out as they come.
pub(crate) async fn connection<S>(stream: S, tools: AppToolSender)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (read, write) = tokio::io::split(stream);
    let mut reader = BufReader::new(read);
    let mut line = Vec::new();
    let Some(session) = session_line(&mut reader, &mut line).await else {
        tracing::warn!("app tools: connection without a session line, closed");
        return;
    };
    tracing::debug!(session = session.0, "app tools: bridge connected");
    let (out, queue) = unbounded_channel();
    tokio::spawn(write_messages(write, queue, session));
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) => {
                tracing::debug!(session = session.0, %error, "app tools: read failed");
                break;
            }
        }
        let sent = match protocol::handle(&line) {
            Reply::Nothing => Ok(()),
            Reply::Send(message) => out.send(message),
            Reply::Call { id, call } => {
                tokio::spawn(answer(session, id, call, tools.clone(), out.clone()));
                Ok(())
            }
        };
        if sent.is_err() {
            break;
        }
    }
    tracing::debug!(session = session.0, "app tools: bridge disconnected");
}

async fn session_line<R>(reader: &mut BufReader<R>, line: &mut Vec<u8>) -> Option<AgentSessionId>
where
    R: AsyncRead + Unpin,
{
    reader.read_until(b'\n', line).await.ok()?;
    let hello: Value = serde_json::from_slice(line).ok()?;
    hello.get("session")?.as_i64().map(AgentSessionId)
}

async fn write_messages<W>(
    mut write: WriteHalf<W>,
    mut queue: UnboundedReceiver<Value>,
    session: AgentSessionId,
) where
    W: AsyncWrite,
{
    while let Some(message) = queue.recv().await {
        let mut line = message.to_string();
        line.push('\n');
        let written = match write.write_all(line.as_bytes()).await {
            Ok(()) => write.flush().await,
            Err(error) => Err(error),
        };
        if let Err(error) = written {
            tracing::debug!(session = session.0, %error, "app tools: write failed");
            return;
        }
    }
    if let Err(error) = write.shutdown().await {
        tracing::debug!(session = session.0, %error, "app tools: close failed");
    }
}

async fn answer(
    session: AgentSessionId,
    id: Value,
    call: AppToolCall,
    tools: AppToolSender,
    out: UnboundedSender<Value>,
) {
    let (reply, outcome) = oneshot::channel();
    let request = AppToolRequest {
        session,
        call,
        reply,
    };
    let outcome = if tools.send(request).is_ok() {
        outcome
            .await
            .unwrap_or_else(|_| Err(NOT_RUNNING.to_owned()))
    } else {
        Err(NOT_RUNNING.to_owned())
    };
    if out.send(protocol::tool_result(id, outcome)).is_err() {
        tracing::debug!(
            session = session.0,
            "app tools: bridge gone before the answer"
        );
    }
}

#[cfg(test)]
mod tests;
