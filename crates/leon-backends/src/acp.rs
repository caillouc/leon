//! Copilot CLI over ACP, the Agent Client Protocol: JSON-RPC
//! 2.0, one message per line on the CLI's stdio; Leon is the client.
//! Protocol version 1, schema v1.32.0 and the protocol pages on
//! agentclientprotocol.com, checked on 2026-11-17. The CLI was not run: the
//! tests replay the spec's own examples.
//!
//! [`session::Session`] is the protocol state machine, free of I/O;
//! [`start`] wires it to the process.

mod permission;
mod session;
mod setup;
mod turn;

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::Path;

use leon_core::ports::{BackendChannel, PortError, SessionSpec};
use leon_domain::{Capability, McpServer};
use serde_json::json;

use crate::conn::{Holders, Launch, Opening};
use crate::options::copilot_flags;
use crate::rpc;
use session::Session;

/// How to start one ACP CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cli {
    /// Name in messages.
    label: &'static str,
    program: &'static str,
    args: Vec<String>,
    /// Environment variables added for it.
    env: Vec<(String, String)>,
}

/// `copilot --acp --stdio` (Copilot CLI docs, ACP server page and command
/// reference, 2026-11-17; ACP is a public preview there). Model, effort and
/// permission are server flags, applied to every session it starts.
/// ⚠️ `--model`, the permission flags and `--allow-tool` are documented for
/// the CLI, not verified with `--acp`.
///
/// `skills` is the folder of Leon's generic skills made for it (EXT-5), a
/// plugin folder (`--plugin-dir`, checked with `copilot skill list` on
/// 1.0.113, 2026-11-18; ⚠️ not with `--acp`).
pub(crate) fn copilot(spec: &SessionSpec, skills: Option<&Path>) -> Cli {
    let s = &spec.settings;
    let mut args = vec!["--acp".to_owned(), "--stdio".to_owned()];
    if let Some(model) = &s.model {
        args.push(format!("--model={model}"));
    }
    let refuses_effort = s
        .model
        .as_deref()
        .is_some_and(|m| crate::options::COPILOT_NO_EFFORT.contains(&m));
    if let Some(effort) = s.effort.as_ref().filter(|_| !refuses_effort) {
        args.push(format!("--effort={effort}"));
    }
    args.extend(copilot_flags(&s.permission).iter().map(|f| (*f).to_owned()));
    // Leon's own tools (P10) run without asking (I6): `SERVER-NAME` allows
    // every tool of that MCP server.
    let trusted: Vec<&str> = spec
        .mcp_servers
        .iter()
        .filter(|m| m.trusted)
        .map(|m| m.name.as_str())
        .collect();
    if !trusted.is_empty() {
        args.push(format!("--allow-tool={}", trusted.join(",")));
    }
    // What the agent file's `tools` leaves out (EXT-7), as permission rules
    // (`copilot help permissions`, 1.0.113): there is none for starting an
    // agent or keeping a task list.
    if let Some(tools) = spec.agent.as_ref().and_then(|a| a.tools.as_deref()) {
        let rules = [
            (Capability::Read, "read"),
            (Capability::Edit, "write"),
            (Capability::Execute, "shell"),
            (Capability::Web, "url"),
        ];
        for (capability, rule) in rules {
            if !tools.contains(&capability) {
                args.push(format!("--deny-tool={rule}"));
            }
        }
    }
    // Leon's own tools (P10), then the source folders' servers (EXT-2), a
    // name given twice once: as this session's extra MCP config, which takes
    // stdio servers (checked with `copilot mcp list`, 1.0.114, 2026-11-18).
    let mut seen = HashSet::new();
    let stdio: Vec<McpServer> = spec
        .mcp_servers
        .iter()
        .chain(spec.sources.iter().flat_map(|s| &s.mcp_servers))
        .filter(|m| seen.insert(m.name.as_str()))
        .cloned()
        .collect();
    if !stdio.is_empty() {
        let config = json!({ "mcpServers": crate::mcp::servers(&stdio) });
        args.push(format!("--additional-mcp-config={config}"));
    }
    if let Some(dir) = skills {
        args.push(format!("--plugin-dir={}", dir.display()));
    }
    // It reads `~/.agents/AGENTS.md` itself from that folder (EXT-8;
    // `copilot instruction list`, 1.0.113, 2026-11-18; ⚠️ not with `--acp`).
    let env = spec
        .instructions
        .iter()
        .filter_map(|i| i.file.parent())
        .map(|dir| {
            (
                "COPILOT_CUSTOM_INSTRUCTIONS_DIRS".to_owned(),
                dir.display().to_string(),
            )
        })
        .collect();
    Cli {
        label: "Copilot CLI",
        program: "copilot",
        args,
        env,
    }
}

/// Starts the CLI for `spec`, under a holder when `holders` is set (I24).
pub(crate) fn start(
    spec: &SessionSpec,
    cli: &Cli,
    path: Option<&OsString>,
    holders: Option<&Holders>,
) -> Result<BackendChannel, PortError> {
    let (session, first) = Session::new(spec, cli.label);
    let launch = Launch {
        path,
        program: cli.program,
        args: &cli.args,
        env: &cli.env,
        cwd: &spec.working_dir,
    };
    let opening = crate::conn::open(holders, &spec.conversation.0.to_string(), &launch)
        .map_err(|e| PortError::new("starting a CLI", format!("{}: {e}", cli.label)))?;
    Ok(rpc::session(opening, cli.label, session, first))
}

/// The CLI found again under its holder (I24): already set up, in its
/// session; nothing to send first.
pub(crate) fn attach(opening: Opening, spec: &SessionSpec, cli: &Cli) -> BackendChannel {
    rpc::session(
        opening,
        cli.label,
        Session::attached(spec, cli.label),
        Vec::new(),
    )
}

impl rpc::Protocol for Session {
    fn handle(
        &mut self,
        msg: &serde_json::Value,
    ) -> (Vec<leon_core::ports::BackendEvent>, Vec<serde_json::Value>) {
        Session::handle(self, msg)
    }
    fn command(&mut self, command: leon_core::ports::BackendCommand) -> Vec<serde_json::Value> {
        Session::command(self, command)
    }
}
