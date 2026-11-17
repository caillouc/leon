//! MCP servers given to a session (P10), in the shape both Claude Code's
//! `--mcp-config` and Codex's `mcp_servers` config use for stdio servers:
//! `{ name: { command, args, env } }`.

use leon_domain::McpServer;
use serde_json::{Map, Value, json};

pub(crate) fn servers(list: &[McpServer]) -> Value {
    let servers: Map<String, Value> = list
        .iter()
        .map(|m| {
            let env: Map<String, Value> =
                m.env.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
            (
                m.name.clone(),
                json!({ "command": m.command.to_string_lossy(), "args": m.args, "env": env }),
            )
        })
        .collect();
    Value::Object(servers)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use leon_domain::McpServer;
    use serde_json::json;

    use super::servers;

    #[test]
    fn stdio_servers_by_name() {
        let list = [McpServer {
            name: "leon".into(),
            command: PathBuf::from("/bin/leon"),
            args: vec!["mcp".into()],
            env: vec![("LEON_SESSION".into(), "7".into())],
        }];
        assert_eq!(
            servers(&list),
            json!({ "leon": { "command": "/bin/leon", "args": ["mcp"], "env": { "LEON_SESSION": "7" } } })
        );
        assert_eq!(servers(&[]), json!({}));
    }
}
