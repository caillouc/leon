//! The app's local MCP server (P10, P15): the tools every conversation gets
//! to start and message other agents, rename its flow and say it
//! moved to another folder.
//!
//! How it is wired: each conversation's CLI starts `leon mcp` (the app's own
//! binary) as a stdio MCP server. That process runs [`bridge`], which pipes
//! bytes between its stdin/stdout and the Unix socket the app listens on with
//! [`serve`]. The app side reads one `{"conversation": <id>}` line written by the
//! bridge, then speaks MCP and sends each tool call to the core as an
//! [`AppToolRequest`](leon_core::ports::AppToolRequest).
//!
//! MCP facts used here — stdio transport (newline-delimited JSON-RPC 2.0, no
//! embedded newlines, nothing else on stdout), `initialize` /
//! `notifications/initialized`, `ping`, `tools/list`, `tools/call` and their
//! JSON shapes — come from the spec, version 2025-06-18, read at
//! <https://modelcontextprotocol.io/specification/2025-06-18> on 2026-11-17.
//! What Claude Code and Codex actually send is ⚠️ not verified against
//! recorded traffic.
//!
//! Must not: write anything but MCP messages to the bridge's stdout; act on a
//! call itself (the core decides); or depend on another adapter.

mod bridge;
mod protocol;
mod server;
mod tools;

use std::path::PathBuf;

use leon_core::ports::AppTools;
use leon_domain::{ConversationId, McpServer};

pub use bridge::bridge;
pub use server::serve;

/// Name of the MCP server; agents see its tools under it.
pub const SERVER_NAME: &str = "leon";

/// Environment variable telling the bridge where the app's socket is.
const SOCKET_VAR: &str = "LEON_SOCKET";
/// Environment variable telling the bridge which conversation it serves.
const CONVERSATION_VAR: &str = "LEON_SESSION";

/// The [`AppTools`] port: every conversation's CLI starts `<command> mcp`, which
/// reaches the app through `socket`.
#[derive(Debug, Clone)]
pub struct LocalTools {
    command: PathBuf,
    socket: PathBuf,
}

impl LocalTools {
    /// `command` is the app's own binary; `socket` is where [`serve`] listens.
    pub fn new(command: PathBuf, socket: PathBuf) -> Self {
        Self { command, socket }
    }
}

impl AppTools for LocalTools {
    fn doctor(&self) -> leon_domain::DoctorCheck {
        use leon_domain::{DoctorCheck, Health};
        let name = "Tools for agents";
        match std::os::unix::net::UnixStream::connect(&self.socket) {
            Ok(_) => DoctorCheck::new(
                Health::Ok,
                name,
                format!("listening on {}", self.socket.display()),
            ),
            Err(error) => DoctorCheck::new(
                Health::Problem,
                name,
                format!("{}: {error}", self.socket.display()),
            )
            .fix("Agents cannot start other agents. Quit every Leon and open it again"),
        }
    }

    fn server(&self, conversation: ConversationId) -> McpServer {
        McpServer {
            name: SERVER_NAME.to_owned(),
            command: self.command.clone(),
            args: vec!["mcp".to_owned()],
            env: vec![
                (
                    SOCKET_VAR.to_owned(),
                    self.socket.to_string_lossy().into_owned(),
                ),
                (CONVERSATION_VAR.to_owned(), conversation.0.to_string()),
            ],
            trusted: true,
        }
    }

    fn instructions(&self) -> &'static str {
        tools::INSTRUCTIONS
    }
}

/// What went wrong while serving or bridging.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The bridge was started without one of its variables.
    #[error("{0} is not set: `leon mcp` is started by a coding CLI inside Leon")]
    MissingVariable(&'static str),
    /// `LEON_SESSION` is not a conversation id.
    #[error("{CONVERSATION_VAR} is not a conversation id: {0:?}")]
    BadConversation(String),
    /// The folder for the socket could not be created.
    #[error("could not create {}", .dir.display())]
    CreateDir {
        /// The folder.
        dir: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// Another Leon is already listening on the socket.
    #[error("another Leon is already running (it listens on {})", .socket.display())]
    InUse {
        /// The socket path.
        socket: PathBuf,
    },
    /// A socket file left by an earlier run could not be removed.
    #[error("could not remove the old socket {}", .socket.display())]
    RemoveStale {
        /// The socket path.
        socket: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The app could not listen on its socket.
    #[error("could not listen on {}", .socket.display())]
    Bind {
        /// The socket path.
        socket: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The bridge could not reach the app.
    #[error("conversation {conversation}: could not connect to Leon at {}", .socket.display())]
    Connect {
        /// The calling conversation.
        conversation: i64,
        /// The socket path.
        socket: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// The bridge lost its stdin, stdout or socket.
    #[error("conversation {conversation}: the MCP bridge failed")]
    Pipe {
        /// The calling conversation.
        conversation: i64,
        /// Why.
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use leon_core::ports::AppTools;
    use leon_domain::{ConversationId, McpServer};

    use super::LocalTools;

    #[test]
    fn server_runs_the_bridge_with_socket_and_conversation() {
        let tools = LocalTools::new(PathBuf::from("/app/leon"), PathBuf::from("/run/leon.sock"));
        assert_eq!(
            tools.server(ConversationId(42)),
            McpServer {
                name: "leon".to_owned(),
                command: PathBuf::from("/app/leon"),
                args: vec!["mcp".to_owned()],
                env: vec![
                    ("LEON_SOCKET".to_owned(), "/run/leon.sock".to_owned()),
                    ("LEON_SESSION".to_owned(), "42".to_owned()),
                ],
                trusted: true,
            }
        );
    }
}
