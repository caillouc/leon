//! What each CLI has as MCP servers and plugins, for the Tools view, read
//! with the CLI's own commands (checked on 2026-11-17): `claude mcp list`
//! and `claude plugin list` (Claude Code 2.1.288, text), `codex mcp list
//! --json` (Codex 0.160.0). `claude mcp list` connects to every server to
//! report its health, so it takes a few seconds.

use std::ffi::OsString;
use std::time::Duration;

use leon_domain::{BackendKind, DoctorCheck, DoctorSection, Health};
use serde_json::Value;

use crate::process::{RunError, run};

const TIMEOUT: Duration = Duration::from_secs(60);

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

pub(crate) fn section(path: Option<&OsString>, backend: BackendKind) -> DoctorSection {
    let title = backend.display_name().to_owned();
    let checks = match backend {
        BackendKind::ClaudeCode => {
            let mut checks = listed(path, "claude", &["mcp", "list"], claude_servers);
            checks.extend(listed(path, "claude", &["plugin", "list"], claude_plugins));
            checks
        }
        BackendKind::Codex => listed(path, "codex", &["mcp", "list", "--json"], codex_servers),
        BackendKind::CopilotCli => vec![DoctorCheck::new(
            Health::Info,
            "MCP servers",
            "Leon cannot list Copilot CLI's servers yet: use /mcp in Copilot CLI",
        )],
    };
    DoctorSection { title, checks }
}

fn listed(
    path: Option<&OsString>,
    program: &str,
    args: &[&str],
    read: fn(&str) -> Vec<DoctorCheck>,
) -> Vec<DoctorCheck> {
    match run(path, program, &strings(args), TIMEOUT) {
        Ok(out) if out.ok => read(&out.stdout),
        Ok(out) => vec![DoctorCheck::new(
            Health::Problem,
            format!("{program} {}", args.join(" ")),
            out.stderr.lines().last().unwrap_or("failed").to_owned(),
        )],
        Err(RunError::NotFound) => vec![DoctorCheck::new(
            Health::Info,
            "Installed",
            format!("`{program}` not found"),
        )],
        Err(error) => vec![DoctorCheck::new(
            Health::Problem,
            format!("{program} {}", args.join(" ")),
            format!("{error:?}"),
        )],
    }
}

/// `claude mcp list`: `name: target - <mark> <status>` per server.
fn claude_servers(out: &str) -> Vec<DoctorCheck> {
    let servers: Vec<DoctorCheck> = out
        .lines()
        .filter_map(|line| {
            let (head, status) = line.rsplit_once(" - ")?;
            let (name, _target) = head.split_once(": ")?;
            let status = status.trim();
            let health = match status.chars().next() {
                Some('✔') => Health::Ok,
                Some('!' | '⏸') => Health::Warning,
                Some('✗' | '✘') => Health::Problem,
                _ => Health::Info,
            };
            let text = status
                .trim_start_matches(|c: char| !c.is_alphabetic())
                .to_owned();
            let check = DoctorCheck::new(health, name.trim(), text);
            Some(
                if health == Health::Warning && status.contains("authentication") {
                    check.fix("Log in once from Claude Code: run `claude`, then /mcp")
                } else {
                    check
                },
            )
        })
        .collect();
    if servers.is_empty() {
        vec![DoctorCheck::new(Health::Info, "MCP servers", "none")]
    } else {
        servers
    }
}

/// `claude plugin list`: blocks of `❯ name`, then `Status: <mark> state`.
fn claude_plugins(out: &str) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let mut name: Option<String> = None;
    for line in out.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix('❯') {
            name = Some(rest.trim().to_owned());
        } else if let Some(status) = line.strip_prefix("Status:")
            && let Some(plugin) = name.take()
        {
            let enabled = status.contains("enabled");
            let state = status
                .trim_start_matches(|c: char| !c.is_alphabetic())
                .trim();
            let health = if enabled { Health::Ok } else { Health::Info };
            checks.push(DoctorCheck::new(
                health,
                format!("Plugin {plugin}"),
                state.to_owned(),
            ));
        }
    }
    checks
}

/// `codex mcp list --json`: `[{ name, enabled, auth_status, … }]`.
fn codex_servers(out: &str) -> Vec<DoctorCheck> {
    let Ok(Value::Array(servers)) = serde_json::from_str::<Value>(out) else {
        return vec![DoctorCheck::new(
            Health::Warning,
            "MCP servers",
            "could not read Codex's list",
        )];
    };
    if servers.is_empty() {
        return vec![DoctorCheck::new(Health::Info, "MCP servers", "none")];
    }
    servers
        .iter()
        .map(|s| {
            let name = s["name"].as_str().unwrap_or("?");
            match (s["enabled"].as_bool(), s["auth_status"].as_str()) {
                (Some(false), _) => DoctorCheck::new(Health::Info, name, "disabled"),
                (_, Some("not_logged_in")) => {
                    DoctorCheck::new(Health::Warning, name, "needs login")
                        .fix(format!("Run `codex mcp login {name}` in a terminal"))
                }
                _ => DoctorCheck::new(Health::Ok, name, "enabled"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use leon_domain::Health;

    use super::{claude_plugins, claude_servers, codex_servers};

    fn names(checks: &[leon_domain::DoctorCheck]) -> Vec<(String, Health, String)> {
        checks
            .iter()
            .map(|c| (c.name.clone(), c.health, c.detail.clone()))
            .collect()
    }

    #[test]
    fn recorded_listings_are_read() {
        let claude = claude_servers(include_str!("../fixtures/claude/2.1.288/mcp-list.txt"));
        assert_eq!(
            names(&claude),
            [
                ("claude.ai Slack".into(), Health::Ok, "Connected".into()),
                (
                    "claude.ai Mixpanel".into(),
                    Health::Warning,
                    "Needs authentication".into()
                ),
            ]
        );
        assert!(claude[1].fix.is_some());
        let plugins = claude_plugins(include_str!("../fixtures/claude/2.1.288/plugin-list.txt"));
        assert_eq!(
            names(&plugins),
            [(
                "Plugin team-kit@team-marketplace".into(),
                Health::Ok,
                "enabled".into()
            )]
        );
        let codex = codex_servers(include_str!("../fixtures/codex/0.160.0/mcp-list.json"));
        assert_eq!(
            names(&codex),
            [
                ("codex_app".into(), Health::Info, "disabled".into()),
                ("cua_repl".into(), Health::Ok, "enabled".into()),
                ("node_repl".into(), Health::Ok, "enabled".into()),
            ]
        );
        assert_eq!(
            names(&codex_servers("[]")),
            [("MCP servers".into(), Health::Info, "none".into())]
        );
    }
}
