//! The user's source folders (P3, EXT-2): MCP servers from `.mcp.json`, in
//! the shape Claude Code's plugins use:
//! `{ "mcpServers": { name: { command, args?, env? } } }`. Servers over HTTP
//! (`url`) are left out: Leon passes stdio servers only for now.

use std::path::Path;

use leon_core::ports::Source;
use leon_domain::McpServer;
use serde_json::Value;

pub(crate) fn read(dir: &Path) -> Source {
    Source {
        dir: dir.to_owned(),
        mcp_servers: mcp_servers(dir),
    }
}

fn mcp_servers(dir: &Path) -> Vec<McpServer> {
    let file = dir.join(".mcp.json");
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<Value>(&text) else {
        tracing::warn!(file = %file.display(), "not valid JSON, ignored");
        return Vec::new();
    };
    let Some(servers) = json["mcpServers"].as_object() else {
        return Vec::new();
    };
    servers
        .iter()
        .filter_map(|(name, server)| {
            let Some(command) = server["command"].as_str() else {
                tracing::warn!(file = %file.display(), server = name, "not a stdio server, ignored");
                return None;
            };
            let strings = |v: &Value| -> Vec<String> {
                v.as_array().map_or_default(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect())
            };
            let env = server["env"].as_object().map_or_default(|e| {
                e.iter()
                    .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
                    .collect()
            });
            Some(McpServer {
                name: name.clone(),
                command: command.into(),
                args: strings(&server["args"]),
                env,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use leon_domain::McpServer;

    use super::read;

    #[test]
    fn stdio_servers_come_from_mcp_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".mcp.json"),
            r#"{"mcpServers":{
                "files":{"command":"npx","args":["-y","files-mcp"],"env":{"ROOT":"/tmp"},"extra":1},
                "remote":{"type":"http","url":"https://example.invalid/mcp"}
            }}"#,
        )
        .unwrap();
        let source = read(dir.path());
        assert_eq!(
            source.mcp_servers,
            vec![McpServer {
                name: "files".into(),
                command: PathBuf::from("npx"),
                args: vec!["-y".into(), "files-mcp".into()],
                env: vec![("ROOT".into(), "/tmp".into())],
            }]
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(read(empty.path()).mcp_servers, Vec::new());
    }
}
