//! MCP servers given to a session (P10), in the shape both Claude Code's
//! `--mcp-config` and Codex's `mcp_servers` config use for stdio servers:
//! `{ name: { command, args, env } }`.

use leon_domain::McpServer;
use serde_json::Value;

pub(crate) fn servers(list: &[McpServer]) -> Value {
    leon_domain::mcp_servers_map(list)
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
            trusted: true,
        }];
        assert_eq!(
            servers(&list),
            json!({ "leon": { "command": "/bin/leon", "args": ["mcp"], "env": { "LEON_SESSION": "7" } } })
        );
        assert_eq!(servers(&[]), json!({}));
    }
}
