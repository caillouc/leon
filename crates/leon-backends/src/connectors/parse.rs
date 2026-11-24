//! Readers for the CLIs' list commands, on output recorded on 2026-11-18 in
//! `fixtures/<cli>/<version>/`. Unknown fields and lines are ignored; a JSON
//! listing without the expected top-level shape gives `None`. What a CLI
//! ships or turns on by itself is left out: only what the user installed or
//! connected is listed.

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

/// `claude plugin list --json`: `[{id, version, enabled, scope, …}]`, `id`
/// being `name@marketplace`. Only installed plugins are listed: no built-in
/// one appears (2.1.288, checked on 2026-11-18).
pub(super) fn claude_plugins(out: &str) -> Option<Vec<Connector>> {
    let list = json(out)?;
    let plugins = list.as_array()?.iter();
    Some(
        plugins
            .filter_map(|p| Some(plugin(p["id"].as_str()?, p)))
            .collect(),
    )
}

/// Codex's marketplaces of bundled plugins, not picked by the user: names
/// Codex reserves (its `marketplace_policy.rs`, read on 2026-11-18), kept in
/// Codex's own folders (`.tmp/bundled-marketplaces`, `codex-runtimes`). A
/// fresh Codex home has none (0.160.0, checked on 2026-11-18): they come with
/// the vendor's desktop app (⚠️ not verified which program adds them).
const CODEX_BUNDLED: [&str; 3] = [
    "openai-bundled",
    "openai-bundled-alpha",
    "openai-primary-runtime",
];

/// A Codex plugin the user did not install: one installed for every account
/// (`installPolicy: INSTALLED_BY_DEFAULT`) or one from a bundled marketplace.
fn codex_default(plugin: &Value) -> bool {
    text(plugin, "installPolicy") == "INSTALLED_BY_DEFAULT"
        || CODEX_BUNDLED.contains(&text(plugin, "marketplaceName"))
}

/// `codex plugin list --json`: `{installed: [{pluginId, marketplaceName,
/// version, enabled, installPolicy, …}], available: […]}`; only `installed`
/// is read.
pub(super) fn codex_plugins(out: &str) -> Option<Vec<Connector>> {
    let list = json(out)?;
    let plugins = list["installed"].as_array()?.iter();
    Some(
        plugins
            .filter(|p| !codex_default(p))
            .filter_map(|p| Some(plugin(p["pluginId"].as_str()?, p)))
            .collect(),
    )
}

/// Copilot CLI's own plugins and servers: `"source": "builtin"`; a server the
/// user added says `user` (1.0.113, recorded on 2026-11-18). `copilot mcp
/// list` groups servers as User, Workspace, Plugin and Builtin (its code).
fn copilot_builtin(item: &Value) -> bool {
    text(item, "source") == "builtin"
}

/// `copilot plugin list --json`: `[{name, marketplace, version, enabled,
/// source}]`; built-in plugins have an empty marketplace.
pub(super) fn copilot_plugins(out: &str) -> Option<Vec<Connector>> {
    let list = json(out)?;
    let plugins = list.as_array()?.iter();
    Some(
        plugins
            .filter(|p| !copilot_builtin(p))
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

/// `codex --disable plugins mcp list --json` (the user's own servers, not
/// the ones plugins bring): `[{name, enabled, auth_status, transport:
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
            .filter(|(_, s)| !copilot_builtin(s))
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

/// Where the account connectors Anthropic provides itself live. Claude Code's
/// docs: "Anthropic also provides some connectors itself, without you or an
/// admin adding them", e.g. Claude Docs (code.claude.com/docs/en/mcp, read on
/// 2026-11-18). Recorded that day, the three on this host were Claude Docs,
/// Claude Data and Meetings; every connector the user added is on its
/// vendor's host.
const ANTHROPIC_CONNECTORS: &str = "https://api.anthropic.com/";

/// Account connectors on every claude.ai account, connected without the user
/// adding or logging in to them, that `claude mcp list` does not mark as such:
/// by their hosts, as listed on 2026-11-24 (Claude Code 2.1.288). Asked by the
/// maintainer: "if this info cannot be retrieved dynamically, you can hard
/// code it".
const CLAUDE_DEFAULT_CONNECTORS: [&str; 3] = [
    "https://mcp.accuweather.com/",
    "https://demandapi-mcp.booking.com/",
    "https://mcp.uber.com/",
];

/// `claude mcp list` (text only): one `name: target - <mark> status` line
/// per server, the status sometimes followed by ` — <error>`. Names starting
/// with `claude.ai ` are the account's connectors; the ones Anthropic provides
/// itself are left out. Other lines are ignored.
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
        Some(_)
            if target.starts_with(ANTHROPIC_CONNECTORS)
                || CLAUDE_DEFAULT_CONNECTORS
                    .iter()
                    .any(|h| target.starts_with(h)) =>
        {
            return None;
        }
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
