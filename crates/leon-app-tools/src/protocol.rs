//! MCP over JSON-RPC 2.0, free of I/O: one incoming line → what to answer.

use leon_core::ports::AppToolCall;
use serde_json::{Value, json};

use crate::SERVER_NAME;
use crate::tools::{self, ToolError};

/// Protocol version answered when the client sends none.
const DEFAULT_VERSION: &str = "2025-06-18";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What to do with one incoming line.
#[derive(Debug, PartialEq)]
pub(crate) enum Reply {
    /// Nothing to send: a notification, a response, a blank line.
    Nothing,
    /// Send this message.
    Send(Value),
    /// Ask the core, then answer request `id` with [`tool_result`].
    Call {
        /// The request id.
        id: Value,
        /// The tool call.
        call: AppToolCall,
    },
}

/// Handles one line from the client. Never fails: bad input gets a
/// JSON-RPC error or is ignored.
pub(crate) fn handle(line: &[u8]) -> Reply {
    if line.trim_ascii().is_empty() {
        return Reply::Nothing;
    }
    let Ok(message) = serde_json::from_slice::<Value>(line) else {
        return Reply::Send(error(Value::Null, PARSE_ERROR, "Parse error"));
    };
    let id = message.get("id").cloned();
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response to a request: this server sends none, so it is ignored.
        if message.get("result").is_some() || message.get("error").is_some() {
            return Reply::Nothing;
        }
        return Reply::Send(error(
            id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "Invalid Request",
        ));
    };
    // A notification (`notifications/initialized`, a cancellation…) gets no answer.
    let Some(id) = id else {
        return Reply::Nothing;
    };
    let params = message.get("params").unwrap_or(&Value::Null);
    match method {
        "initialize" => Reply::Send(result(id, initialize(params))),
        "ping" => Reply::Send(result(id, json!({}))),
        "tools/list" => Reply::Send(result(id, tools::list())),
        "tools/call" => call(id, params),
        _ => Reply::Send(error(
            id,
            METHOD_NOT_FOUND,
            &format!("Method not found: {method}"),
        )),
    }
}

/// The answer to a `tools/call`, from the core's result.
pub(crate) fn tool_result(id: Value, outcome: Result<String, String>) -> Value {
    let (text, is_error) = match outcome {
        Ok(text) => (text, false),
        Err(text) => (text, true),
    };
    result(
        id,
        json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
    )
}

fn initialize(params: &Value) -> Value {
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
    })
}

fn call(id: Value, params: &Value) -> Reply {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params.get("arguments").unwrap_or(&Value::Null);
    match tools::parse(name, arguments) {
        Ok(call) => Reply::Call { id, call },
        Err(ToolError::Arguments(message)) => Reply::Send(tool_result(id, Err(message))),
        Err(ToolError::Unknown) => {
            Reply::Send(error(id, INVALID_PARAMS, &format!("Unknown tool: {name}")))
        }
    }
}

fn result(id: Value, result: Value) -> Value {
    response(id, "result", result)
}

fn error(id: Value, code: i64, message: &str) -> Value {
    response(id, "error", json!({ "code": code, "message": message }))
}

fn response(id: Value, key: &str, body: Value) -> Value {
    Value::Object(
        [
            ("jsonrpc".to_owned(), Value::from("2.0")),
            ("id".to_owned(), id),
            (key.to_owned(), body),
        ]
        .into_iter()
        .collect(),
    )
}

#[cfg(test)]
mod tests {
    use leon_core::ports::AppToolCall;
    use serde_json::{Value, json};

    use super::{Reply, handle};

    fn send(line: &str) -> Value {
        match handle(line.as_bytes()) {
            Reply::Send(message) => message,
            other => panic!("{line}: {other:?}"),
        }
    }

    #[test]
    fn initialize_echoes_the_client_version() {
        let answer = send(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"c","version":"1"}}}"#,
        );
        assert_eq!(answer["id"], 1);
        assert_eq!(answer["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(answer["result"]["capabilities"], json!({ "tools": {} }));
        assert_eq!(answer["result"]["serverInfo"]["name"], "leon");
    }

    #[test]
    fn requests_get_results_or_errors() {
        let cases: &[(&str, Value)] = &[
            (
                r#"{"jsonrpc":"2.0","id":"a","method":"ping"}"#,
                json!({ "jsonrpc": "2.0", "id": "a", "result": {} }),
            ),
            (
                r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#,
                json!({ "jsonrpc": "2.0", "id": 2, "error": { "code": -32601, "message": "Method not found: resources/list" } }),
            ),
            (
                "{not json",
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "Parse error" } }),
            ),
            (
                "[1, 2]",
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32600, "message": "Invalid Request" } }),
            ),
            (
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
                json!({ "jsonrpc": "2.0", "id": 3, "error": { "code": -32602, "message": "Unknown tool: nope" } }),
            ),
            (
                r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"rename_flow"}}"#,
                json!({ "jsonrpc": "2.0", "id": 4, "result": {
                    "content": [{ "type": "text", "text": "`title` is required: a non-empty string" }],
                    "isError": true,
                } }),
            ),
        ];
        for (line, expected) in cases {
            assert_eq!(&send(line), expected, "{line}");
        }
    }

    #[test]
    fn tools_list_has_the_four_tools() {
        let answer = send(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","extra":true}"#);
        let names: Vec<_> = answer["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "start_agent",
                "message_agent",
                "rename_flow",
                "set_working_directory"
            ]
        );
    }

    #[test]
    fn notifications_responses_and_blank_lines_get_nothing() {
        for line in [
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#,
            r#"{"jsonrpc":"2.0","id":9,"result":{}}"#,
            "  \r\n",
        ] {
            assert_eq!(handle(line.as_bytes()), Reply::Nothing, "{line}");
        }
    }

    #[test]
    fn a_valid_call_goes_to_the_core() {
        assert_eq!(
            handle(
                br#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"rename_flow","arguments":{"title":"New"}}}"#
            ),
            Reply::Call {
                id: json!(5),
                call: AppToolCall::RenameFlow { title: "New".to_owned() },
            }
        );
    }
}
