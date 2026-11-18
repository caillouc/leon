//! The `leon mcp` side: a stdio MCP server for the CLI that only pipes bytes
//! to and from the app's socket. Blocking, std only.

use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use crate::{Error, SESSION_VAR, SOCKET_VAR};

/// Runs the bridge for the session in `LEON_SESSION`, through the socket in
/// `LEON_SOCKET`. Returns when the app closes the connection. Writes nothing
/// but the app's messages to stdout.
pub fn bridge() -> Result<(), Error> {
    let (socket, session) = target(std::env::var_os(SOCKET_VAR), std::env::var_os(SESSION_VAR))?;
    let stream = UnixStream::connect(&socket).map_err(|source| Error::Connect {
        session,
        socket,
        source,
    })?;
    pipe(session, std::io::stdin(), &mut std::io::stdout(), &stream)
}

/// The socket path and session id, from the two variables' values.
fn target(socket: Option<OsString>, session: Option<OsString>) -> Result<(PathBuf, i64), Error> {
    let socket = socket
        .filter(|socket| !socket.is_empty())
        .ok_or(Error::MissingVariable(SOCKET_VAR))?;
    let session = session.ok_or(Error::MissingVariable(SESSION_VAR))?;
    let session = session.to_string_lossy();
    let id = session
        .parse()
        .map_err(|_| Error::BadSession(session.clone().into_owned()))?;
    Ok((PathBuf::from(socket), id))
}

/// Says which session this is, then copies `input` to the app on a thread
/// (closing the write side at its end) and the app's bytes to `output` here,
/// until the app closes.
fn pipe<I, O>(session: i64, mut input: I, output: &mut O, stream: &UnixStream) -> Result<(), Error>
where
    I: Read + Send + 'static,
    O: Write,
{
    let fail = |source: std::io::Error| Error::Pipe { session, source };
    let mut to_app = stream.try_clone().map_err(fail)?;
    to_app
        .write_all(format!("{{\"session\":{session}}}\n").as_bytes())
        .map_err(fail)?;
    std::thread::spawn(move || {
        if let Err(error) = std::io::copy(&mut input, &mut to_app) {
            tracing::debug!(session, %error, "MCP bridge: input failed");
        }
        if let Err(error) = to_app.shutdown(Shutdown::Write) {
            tracing::debug!(session, %error, "MCP bridge: close failed");
        }
    });
    copy_flushing(&mut &*stream, output).map_err(fail)
}

/// Copies until end of input, flushing after every read so each message
/// reaches the CLI at once.
fn copy_flushing(from: &mut impl Read, to: &mut impl Write) -> std::io::Result<()> {
    let mut buffer = [0; 8192];
    loop {
        let read = match from.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        to.write_all(&buffer[..read])?;
        to.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::io::{BufRead, BufReader, Cursor, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::time::Duration;

    use leon_core::ports::AppToolCall;
    use leon_domain::AgentSessionId;
    use serde_json::{Value, json};

    use super::{pipe, target};

    /// Socket and session variables, then the expected path and id or error.
    type Case = (
        Option<OsString>,
        Option<OsString>,
        Result<(&'static str, i64), &'static str>,
    );

    #[test]
    fn target_reads_socket_and_session() {
        let os = |value: &str| Some(OsString::from(value));
        let cases: &[Case] = &[
            (os("/run/leon.sock"), os("12"), Ok(("/run/leon.sock", 12))),
            (None, os("12"), Err("LEON_SOCKET is not set")),
            (os(""), os("12"), Err("LEON_SOCKET is not set")),
            (os("/run/leon.sock"), None, Err("LEON_SESSION is not set")),
            (
                os("/run/leon.sock"),
                os("twelve"),
                Err("LEON_SESSION is not a session id: \"twelve\""),
            ),
        ];
        for (socket, session, expected) in cases {
            let got = target(socket.clone(), session.clone());
            match (got, expected) {
                (Ok((path, id)), Ok((want_path, want_id))) => {
                    assert_eq!((path, id), (PathBuf::from(want_path), *want_id));
                }
                (Err(error), Err(want)) => assert!(error.to_string().starts_with(want), "{error}"),
                (got, want) => panic!("{socket:?} {session:?}: {got:?}, expected {want:?}"),
            }
        }
    }

    #[test]
    fn pipe_sends_the_session_then_copies_both_ways() {
        let (bridge_end, mut app_end) = UnixStream::pair().unwrap();
        let app = std::thread::spawn(move || {
            let mut received = String::new();
            app_end.read_to_string(&mut received).unwrap();
            app_end.write_all(b"{\"id\":1}\n").unwrap();
            received
        });
        let mut output = Vec::new();
        pipe(
            5,
            Cursor::new(b"{\"id\":1}\n".to_vec()),
            &mut output,
            &bridge_end,
        )
        .unwrap();
        assert_eq!(app.join().unwrap(), "{\"session\":5}\n{\"id\":1}\n");
        assert_eq!(output, b"{\"id\":1}\n");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_bridge_talks_to_the_server() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("leon.sock");
        let (core, mut requests) = tokio::sync::mpsc::unbounded_channel();
        crate::serve(&tokio::runtime::Handle::current(), &socket, core).unwrap();
        tokio::spawn(async move {
            while let Some(request) = requests.recv().await {
                let reply = format!("{:?} from {:?}", request.call, request.session);
                request.reply.send(Ok(reply)).unwrap();
            }
        });
        let input = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "rename_flow", "arguments": { "title": "T" } } }),
        ]
        .map(|message| format!("{message}\n"))
        .concat();
        let bridge = tokio::task::spawn_blocking(move || {
            let mut output = Vec::new();
            let stream = UnixStream::connect(&socket).unwrap();
            pipe(5, Cursor::new(input.into_bytes()), &mut output, &stream).map(|()| output)
        });
        let output = tokio::time::timeout(Duration::from_secs(5), bridge)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let answers: Vec<Value> = BufReader::new(output.as_slice())
            .lines()
            .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
            .collect();
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[0]["result"]["protocolVersion"], "2025-06-18");
        let call = AppToolCall::RenameConversation {
            title: "T".to_owned(),
        };
        assert_eq!(
            answers[1]["result"]["content"][0]["text"],
            format!("{call:?} from {:?}", AgentSessionId(5))
        );
    }
}
