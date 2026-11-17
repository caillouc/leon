//! MCP servers given to agent sessions (P10, EXT-2), and the `.mcp.json`
//! shape every CLI and plugin uses for stdio servers:
//! `{ "mcpServers": { name: { command, args?, env? } } }`.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

/// An MCP server a CLI starts itself and talks to over stdio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    /// Name the agent sees its tools under.
    pub name: String,
    /// Program to run.
    pub command: PathBuf,
    /// Its arguments.
    pub args: Vec<String>,
    /// Extra environment variables.
    pub env: Vec<(String, String)>,
    /// Leon's own tools (P10): their calls run without a permission prompt
    /// (I6). The user's servers keep the CLI's usual prompts.
    pub trusted: bool,
}

impl McpServer {
    /// Its entry in a `.mcp.json` `mcpServers` map.
    pub fn entry(&self) -> Value {
        let env: Map<String, Value> = self
            .env
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        json!({ "command": self.command.to_string_lossy(), "args": self.args, "env": env })
    }

    /// A server from its `.mcp.json` entry; `None` for a server that is not
    /// stdio (e.g. over HTTP).
    pub fn from_entry(name: &str, entry: &Value) -> Option<Self> {
        let command = entry["command"].as_str()?;
        let args = entry["args"].as_array().map_or_default(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_owned))
                .collect()
        });
        let env = entry["env"].as_object().map_or_default(|e| {
            e.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
                .collect()
        });
        Some(Self {
            name: name.to_owned(),
            command: command.into(),
            args,
            env,
            trusted: false,
        })
    }
}

/// A `mcpServers` map, name → entry.
pub fn mcp_servers_map(list: &[McpServer]) -> Value {
    Value::Object(list.iter().map(|m| (m.name.clone(), m.entry())).collect())
}

/// Splits a command line typed by the user into words: spaces separate
/// words, except inside double or single quotes. `None` when a quote is not
/// closed.
pub fn split_command(line: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            (None, c) => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_word {
        words.push(word);
    }
    Some(words)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::{McpServer, mcp_servers_map, split_command};

    #[test]
    fn entries_round_trip_and_skip_http() {
        let server = McpServer {
            name: "files".into(),
            command: PathBuf::from("npx"),
            args: vec!["-y".into(), "files-mcp".into()],
            env: vec![("ROOT".into(), "/tmp".into())],
            trusted: false,
        };
        let map = mcp_servers_map(std::slice::from_ref(&server));
        assert_eq!(
            map,
            json!({ "files": { "command": "npx", "args": ["-y", "files-mcp"], "env": { "ROOT": "/tmp" } } })
        );
        assert_eq!(McpServer::from_entry("files", &map["files"]), Some(server));
        assert_eq!(
            McpServer::from_entry("remote", &json!({ "type": "http", "url": "https://x" })),
            None
        );
    }

    #[test]
    fn command_lines_split_like_a_shell() {
        let cases: [(&str, Option<Vec<&str>>); 5] = [
            ("npx -y @org/server", Some(vec!["npx", "-y", "@org/server"])),
            ("  uvx   tool  ", Some(vec!["uvx", "tool"])),
            (
                r#"node "/My Tools/server.js" --flag"#,
                Some(vec!["node", "/My Tools/server.js", "--flag"]),
            ),
            ("TOKEN='a b' URL=x", Some(vec!["TOKEN=a b", "URL=x"])),
            ("npx \"unclosed", None),
        ];
        for (line, expected) in cases {
            let expected = expected.map(|w| w.into_iter().map(str::to_owned).collect::<Vec<_>>());
            assert_eq!(split_command(line), expected, "{line}");
        }
    }
}
