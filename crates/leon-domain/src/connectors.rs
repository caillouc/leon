//! What each coding CLI can reach beyond the code (TOOLS-1): its plugins,
//! its own MCP servers, and the connectors of the account it is logged in
//! with (claude.ai connectors such as Slack or Gmail).

use crate::BackendKind;

/// Where a connector comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectorKind {
    /// A plugin installed in the CLI.
    Plugin,
    /// An MCP server configured in the CLI itself.
    Server,
    /// A connector of the account the CLI is logged in with.
    Account,
}

/// Whether a connector works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectorState {
    /// Enabled, or connected.
    On,
    /// Installed but disabled.
    Off,
    /// Offered by the account, not logged in yet.
    NeedsLogin,
    /// Configured, but the CLI could not reach it.
    Failed,
}

/// One connector of one CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connector {
    /// Its name, e.g. "Slack" or "github@claude-plugins".
    pub name: String,
    /// Plugin, server or account connector.
    pub kind: ConnectorKind,
    /// Whether it works.
    pub state: ConnectorState,
    /// A short detail: a version, a command, a URL, an error.
    pub detail: String,
}

/// A CLI's connectors, or why they could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliConnectors {
    /// The CLI.
    pub backend: BackendKind,
    /// Its connectors, by kind then name.
    pub connectors: Vec<Connector>,
    /// What went wrong while reading them, if anything (the CLI is missing,
    /// a command failed). Connectors read before the failure are kept.
    pub problem: Option<String>,
}
