//! Readers for the CLIs' list commands, on output recorded on 2026-11-18 in
//! `fixtures/<cli>/<version>/`. Unknown fields and lines are ignored; a JSON
//! listing without the expected top-level shape gives `None`.

use leon_domain::{Connector, ConnectorKind, ConnectorState};
use serde_json::Value;

use crate::process::clip;

/// Longest detail kept: a failure can carry a whole HTTP body.
const DETAIL: usize = 200;

fn connector(name: &str, kind: ConnectorKind, state: ConnectorState, detail: &str) -> Connector {
    Connector {
        name: name.to_owned(),
        kind,
        state,
        detail: clip(detail, DETAIL),
    }
}

fn text<'a>(item: &'a Value, key: &str) -> &'a str {
    item[key].as_str().unwrap_or_default()
}

/// Only `enabled: false` is off, as Codex's own listing reads it.
fn enabled(item: &Value) -> ConnectorState {
    if item["enabled"].as_bool() == Some(false) {
        ConnectorState::Off
    } else {
        ConnectorState::On
    }
}

/// A plugin entry: `enabled` and `version` are named alike by all three CLIs.
fn plugin(name: &str, item: &Value) -> Connector {
    connector(
        name,
        ConnectorKind::Plugin,
        enabled(item),
        text(item, "version"),
    )
}

/// A server's command, or its URL for a remote one.
fn target(item: &Value) -> &str {
    item["command"]
        .as_str()
        .or_else(|| item["url"].as_str())
        .unwrap_or_default()
}

fn json(out: &str) -> Option<Value> {
    serde_json::from_str(out).ok()
}

/// `claude plugin list --json`: `[{id, version, enabled, …}]`, `id` being
/// `name@marketplace`.
pub(super) fn claude_plugins(out: &str) -> Option<Vec<Connector>> {
    let list = json(out)?;
    let plugins = list.as_array()?.iter();
    Some(
        plugins
            .filter_map(|p| Some(plugin(p["id"].as_str()?, p)))
            .collect(),
    )
}

/// `codex plugin list --json`: `{installed: [{pluginId, version, enabled,
/// …}], available: […]}`; only `installed` is read.
pub(super) fn codex_plugins(out: &str) -> Option<Vec<Connector>> {
    let list = json(out)?;
    let plugins = list["installed"].as_array()?.iter();
    Some(
        plugins
            .filter_map(|p| Some(plugin(p["pluginId"].as_str()?, p)))
            .collect(),
    )
}

/// `copilot plugin list --json`: `[{name, marketplace, version, enabled,
/// source}]`; built-in plugins have an empty marketplace.
pub(super) fn copilot_plugins(out: &str) -> Option<Vec<Connector>> {
    let list = json(out)?;
    let plugins = list.as_array()?.iter();
    Some(
        plugins
            .filter_map(|p| {
                let name = p["name"].as_str()?;
                Some(match text(p, "marketplace") {
                    "" => plugin(name, p),
                    marketplace => plugin(&format!("{name}@{marketplace}"), p),
                })
            })
            .collect(),
    )
}

/// `codex mcp list --json`: `[{name, enabled, auth_status, transport:
/// {command | url, …}, …}]`.
pub(super) fn codex_servers(out: &str) -> Option<Vec<Connector>> {
    let list = json(out)?;
    let servers = list.as_array()?.iter();
    Some(
        servers
            .filter_map(|s| {
                let state = match (enabled(s), text(s, "auth_status")) {
                    (ConnectorState::On, "not_logged_in") => ConnectorState::NeedsLogin,
                    (state, _) => state,
                };
                let name = s["name"].as_str()?;
                Some(connector(
                    name,
                    ConnectorKind::Server,
                    state,
                    target(&s["transport"]),
                ))
            })
            .collect(),
    )
}

/// `copilot mcp list --json`: `{mcpServers: {<name>: {type, command | url,
/// source, enabled, …}}}`.
pub(super) fn copilot_servers(out: &str) -> Option<Vec<Connector>> {
    let list = json(out)?;
    let servers = list["mcpServers"].as_object()?.iter();
    Some(
        servers
            .map(|(name, s)| connector(name, ConnectorKind::Server, enabled(s), target(s)))
            .collect(),
    )
}

/// Statuses `claude mcp list` prints after its mark, as written in Claude
/// Code 2.1.288's own code (read on 2026-11-18). The longer one first: it
/// starts like "Connected".
const CLAUDE_STATUSES: [(&str, ConnectorState); 9] = [
    (
        "Connected \u{b7} tools fetch failed",
        ConnectorState::Failed,
    ),
    ("Connected", ConnectorState::On),
    ("Needs authentication", ConnectorState::NeedsLogin),
    ("Failed to connect", ConnectorState::Failed),
    ("Connection error", ConnectorState::Failed),
    ("Not configured", ConnectorState::Failed),
    ("Pending approval", ConnectorState::Off),
    ("Rejected", ConnectorState::Off),
    ("Disabled", ConnectorState::Off),
];

/// `claude mcp list` (text only): one `name: target - <mark> status` line
/// per server, the status sometimes followed by ` — <error>`. Names starting
/// with `claude.ai ` are the account's connectors. Other lines are ignored.
pub(super) fn claude_servers(out: &str) -> Vec<Connector> {
    out.lines().filter_map(claude_server).collect()
}

fn claude_server(line: &str) -> Option<Connector> {
    let (name, rest) = line.split_once(": ")?;
    // The target can hold " - " too: the separator is the first one followed
    // by a known status.
    let (target, status, state) = rest.match_indices(" - ").find_map(|(at, separator)| {
        let status = rest[at + separator.len()..]
            .trim_start_matches(|c: char| !c.is_alphanumeric())
            .trim_end();
        let (_, state) = CLAUDE_STATUSES
            .iter()
            .find(|(known, _)| status.starts_with(known))?;
        Some((&rest[..at], status, *state))
    })?;
    let (kind, name) = match name.strip_prefix("claude.ai ") {
        Some(name) => (ConnectorKind::Account, name),
        None => (ConnectorKind::Server, name),
    };
    let detail = match state {
        ConnectorState::On | ConnectorState::NeedsLogin => target,
        ConnectorState::Off | ConnectorState::Failed => status,
    };
    Some(connector(name.trim(), kind, state, detail.trim()))
}

#[cfg(test)]
mod tests;
