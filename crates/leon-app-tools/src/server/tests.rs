use std::path::PathBuf;
use std::time::Duration;

use leon_core::ports::{AppToolCall, AppToolRequest};
use leon_domain::ConversationId;
use serde_json::{Value, json};
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::{connection, serve};

const WAIT: Duration = Duration::from_secs(5);

/// The bridge's end of a connection.
struct Client {
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    write: WriteHalf<DuplexStream>,
}

impl Client {
    async fn line(&mut self, line: &str) {
        self.write
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
    }

    async fn send(&mut self, message: Value) {
        self.line(&message.to_string()).await;
    }

    async fn call(&mut self, id: i64, name: &str, arguments: Value) {
        self.send(json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments },
        }))
        .await;
    }

    /// The next message, or `None` once the server closed.
    async fn next(&mut self) -> Option<Value> {
        let line = tokio::time::timeout(WAIT, self.lines.next_line())
            .await
            .unwrap()
            .unwrap()?;
        Some(serde_json::from_str(&line).unwrap())
    }
}

/// A connection for `conversation`, and what the core receives from it.
async fn connect(conversation: i64) -> (Client, UnboundedReceiver<AppToolRequest>) {
    let (core, requests) = unbounded_channel();
    let (client, server) = tokio::io::duplex(64 * 1024);
    tokio::spawn(connection(server, core));
    let (read, write) = tokio::io::split(client);
    let mut client = Client {
        lines: BufReader::new(read).lines(),
        write,
    };
    client
        .line(&format!(r#"{{"session": {conversation}}}"#))
        .await;
    (client, requests)
}

async fn request(requests: &mut UnboundedReceiver<AppToolRequest>) -> AppToolRequest {
    tokio::time::timeout(WAIT, requests.recv())
        .await
        .unwrap()
        .unwrap()
}

fn tool_result(id: i64, text: &str, is_error: bool) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": {
        "content": [{ "type": "text", "text": text }], "isError": is_error,
    } })
}

#[tokio::test]
async fn each_tool_call_reaches_the_core_with_the_conversation() {
    let cases = [
        (
            "start_agent",
            json!({
                "agent": "coder", "prompt": "Fix it", "working_directory": "/w/tree",
                "for_user": true,
            }),
            AppToolCall::StartAgent {
                agent: "coder".to_owned(),
                prompt: "Fix it".to_owned(),
                working_directory: Some(PathBuf::from("/w/tree")),
                for_user: true,
            },
        ),
        (
            "message_agent",
            json!({ "conversation": 12, "text": "And the tests?" }),
            AppToolCall::MessageAgent {
                conversation: "12".to_owned(),
                text: "And the tests?".to_owned(),
            },
        ),
        (
            "rename_flow",
            json!({ "title": "Parser fix", "unknown": 1 }),
            AppToolCall::RenameFlow {
                title: "Parser fix".to_owned(),
            },
        ),
        (
            "set_working_directory",
            json!({ "path": "/w/other" }),
            AppToolCall::SetWorkingDirectory {
                path: PathBuf::from("/w/other"),
            },
        ),
    ];
    for (name, arguments, expected) in cases {
        let (mut client, mut requests) = connect(7).await;
        client.call(1, name, arguments).await;
        let request = request(&mut requests).await;
        assert_eq!(
            (request.conversation, &request.call),
            (ConversationId(7), &expected),
            "{name}"
        );
        request.reply.send(Ok(format!("{name} done"))).unwrap();
        assert_eq!(
            client.next().await,
            Some(tool_result(1, &format!("{name} done"), false))
        );
    }
}

#[tokio::test]
async fn an_error_from_the_core_is_a_tool_error() {
    let (mut client, mut requests) = connect(1).await;
    client
        .call(
            4,
            "message_agent",
            json!({ "conversation": 99, "text": "hi" }),
        )
        .await;
    request(&mut requests)
        .await
        .reply
        .send(Err("No conversation 99 in this flow".to_owned()))
        .unwrap();
    assert_eq!(
        client.next().await,
        Some(tool_result(4, "No conversation 99 in this flow", true))
    );
}

#[tokio::test]
async fn a_slow_call_does_not_block_the_next_ones() {
    let (mut client, mut requests) = connect(1).await;
    client
        .call(1, "rename_flow", json!({ "title": "First" }))
        .await;
    client
        .call(2, "rename_flow", json!({ "title": "Second" }))
        .await;
    let first = request(&mut requests).await;
    let second = request(&mut requests).await;
    client
        .send(json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }))
        .await;
    assert_eq!(
        client.next().await,
        Some(json!({ "jsonrpc": "2.0", "id": 3, "result": {} }))
    );
    second.reply.send(Ok("second".to_owned())).unwrap();
    first.reply.send(Ok("first".to_owned())).unwrap();
    assert_eq!(client.next().await, Some(tool_result(2, "second", false)));
    assert_eq!(client.next().await, Some(tool_result(1, "first", false)));
}

#[tokio::test]
async fn core_gone_means_leon_is_not_running() {
    // The core drops the reply without answering.
    let (mut client, mut requests) = connect(1).await;
    client.call(1, "rename_flow", json!({ "title": "T" })).await;
    drop(request(&mut requests).await);
    assert_eq!(
        client.next().await,
        Some(tool_result(1, "Leon is not running", true))
    );
    // The core's receiver is gone.
    drop(requests);
    client.call(2, "rename_flow", json!({ "title": "T" })).await;
    assert_eq!(
        client.next().await,
        Some(tool_result(2, "Leon is not running", true))
    );
}

#[tokio::test]
async fn bad_input_gets_errors_and_the_connection_stays_up() {
    let (mut client, _requests) = connect(1).await;
    client.line("garbage").await;
    assert_eq!(client.next().await.unwrap()["error"]["code"], -32700);
    client
        .send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .await;
    client
        .send(json!({ "jsonrpc": "2.0", "id": 8, "method": "prompts/list" }))
        .await;
    assert_eq!(client.next().await.unwrap()["error"]["code"], -32601);
    client
        .call(9, "start_agent", json!({ "agent": "coder" }))
        .await;
    assert_eq!(
        client.next().await,
        Some(tool_result(
            9,
            "`prompt` is required: a non-empty string",
            true
        ))
    );
}

#[tokio::test]
async fn a_connection_without_a_conversation_line_is_closed() {
    let (core, _requests) = unbounded_channel();
    let (client, server) = tokio::io::duplex(1024);
    tokio::spawn(connection(server, core));
    let (read, write) = tokio::io::split(client);
    let mut client = Client {
        lines: BufReader::new(read).lines(),
        write,
    };
    client
        .line(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#)
        .await;
    assert_eq!(client.next().await, None);
}

#[tokio::test]
async fn closing_the_input_still_delivers_pending_answers() {
    let (mut client, mut requests) = connect(1).await;
    client.call(1, "rename_flow", json!({ "title": "T" })).await;
    let pending = request(&mut requests).await;
    client.write.shutdown().await.unwrap();
    pending.reply.send(Ok("done".to_owned())).unwrap();
    assert_eq!(client.next().await, Some(tool_result(1, "done", false)));
    assert_eq!(client.next().await, None);
}

#[tokio::test]
async fn serve_creates_the_folder_and_replaces_a_stale_socket() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("run").join("leon.sock");
    let (core, _requests) = unbounded_channel();
    serve(&tokio::runtime::Handle::current(), &socket, core.clone()).unwrap();
    let mode = std::fs::metadata(socket.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    // A second Leon finds the first one still listening and stops.
    let error = serve(&tokio::runtime::Handle::current(), &socket, core.clone()).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("another Leon is already running"),
        "{error}"
    );

    // A socket file nobody listens on is left from a crash: it is replaced.
    let stale = temp.path().join("stale.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
    serve(&tokio::runtime::Handle::current(), &stale, core).unwrap();
    assert!(tokio::net::UnixStream::connect(&stale).await.is_ok());
}

#[tokio::test]
async fn serve_refuses_to_replace_a_file_that_is_not_a_socket() {
    let temp = tempfile::tempdir().unwrap();
    let socket = temp.path().join("leon.sock");
    std::fs::write(&socket, "keep me").unwrap();
    let (core, _requests) = unbounded_channel();
    let error = serve(&tokio::runtime::Handle::current(), &socket, core).unwrap_err();
    assert!(
        error.to_string().starts_with("could not listen on"),
        "{error}"
    );
    assert_eq!(std::fs::read_to_string(&socket).unwrap(), "keep me");
}
