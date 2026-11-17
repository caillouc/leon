//! MCP servers given to agent sessions (P10, EXT-2).

use std::path::PathBuf;

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
}
