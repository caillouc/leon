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

/// One service across the CLIs (TOOLS-1): its name, and what each CLI has
/// for it — an account connector, a plugin, a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorRow<'a> {
    /// Its name, as people know it ("Metabase").
    pub name: String,
    /// Each CLI's entries for it.
    pub entries: Vec<(BackendKind, &'a Connector)>,
}

impl ConnectorRow<'_> {
    /// Whether it works in at least one CLI.
    pub fn connected(&self) -> bool {
        self.entries
            .iter()
            .any(|(_, c)| c.state == ConnectorState::On)
    }

    /// What `backend` has for it.
    pub fn of(&self, backend: BackendKind) -> impl Iterator<Item = &Connector> {
        self.entries
            .iter()
            .filter(move |(b, _)| *b == backend)
            .map(|(_, c)| *c)
    }
}

/// The key the entries of two CLIs for the same service share: lower case,
/// letters and digits only, without a `@marketplace` suffix or the words
/// "mcp" and "server" ("Metabase", "metabase@kit", "metabase-mcp").
pub fn connector_key(name: &str) -> String {
    let base = name.split('@').next().unwrap_or(name).to_lowercase();
    let key: String = base
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && *w != "mcp" && *w != "server")
        .collect();
    if key.is_empty() { base } else { key }
}

/// Every CLI's connectors, one row per service, by name. A row is named
/// after an account connector when there is one (people's names), else
/// after the first entry, without its `@marketplace`.
pub fn connector_table(clis: &[CliConnectors]) -> Vec<ConnectorRow<'_>> {
    let mut rows: Vec<(String, ConnectorRow<'_>)> = Vec::new();
    for cli in clis {
        for c in &cli.connectors {
            let key = connector_key(&c.name);
            let display = c.name.split('@').next().unwrap_or(&c.name).to_owned();
            match rows.iter_mut().find(|(k, _)| *k == key) {
                Some((_, row)) => {
                    if c.kind == ConnectorKind::Account {
                        row.name = display;
                    }
                    row.entries.push((cli.backend, c));
                }
                None => rows.push((
                    key,
                    ConnectorRow {
                        name: display,
                        entries: vec![(cli.backend, c)],
                    },
                )),
            }
        }
    }
    let mut rows: Vec<ConnectorRow<'_>> = rows.into_iter().map(|(_, row)| row).collect();
    rows.sort_by_key(|r| r.name.to_lowercase());
    rows
}

#[cfg(test)]
mod tests {
    use super::{
        CliConnectors, Connector, ConnectorKind, ConnectorState, connector_key, connector_table,
    };
    use crate::BackendKind;

    fn connector(name: &str, kind: ConnectorKind, state: ConnectorState) -> Connector {
        Connector {
            name: name.into(),
            kind,
            state,
            detail: String::new(),
        }
    }

    #[test]
    fn the_same_service_in_several_clis_is_one_row() {
        use ConnectorKind::{Account, Plugin, Server};
        use ConnectorState::{NeedsLogin, Off, On};
        let cases = [
            ("Metabase", "metabase"),
            ("metabase@team-kit", "metabase"),
            ("metabase-mcp", "metabase"),
            ("Google Drive", "googledrive"),
            ("mcp", "mcp"),
        ];
        for (name, key) in cases {
            assert_eq!(connector_key(name), key, "{name}");
        }
        let clis = [
            CliConnectors {
                backend: BackendKind::ClaudeCode,
                connectors: vec![
                    connector("Slack", Account, On),
                    connector("Metabase", Account, NeedsLogin),
                ],
                problem: None,
            },
            CliConnectors {
                backend: BackendKind::Codex,
                connectors: vec![
                    connector("metabase-mcp", Server, On),
                    connector("github@openai-curated", Plugin, Off),
                ],
                problem: None,
            },
        ];
        let table = connector_table(&clis);
        let names: Vec<(&str, bool)> = table
            .iter()
            .map(|r| (r.name.as_str(), r.connected()))
            .collect();
        assert_eq!(
            names,
            [("github", false), ("Metabase", true), ("Slack", true)]
        );
        let metabase = &table[1];
        assert_eq!(metabase.of(BackendKind::ClaudeCode).count(), 1);
        assert_eq!(
            metabase.of(BackendKind::Codex).next().map(|c| c.state),
            Some(On)
        );
        assert_eq!(metabase.of(BackendKind::CopilotCli).count(), 0);
    }
}
