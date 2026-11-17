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

use std::ffi::OsString;

use leon_core::ports::{BackendChannel, PortError, SessionSpec};

use crate::options::copilot_flags;
use crate::rpc::{self, Launch};
use session::Session;

/// How to start one ACP CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cli {
    /// Name in messages.
    label: &'static str,
    program: &'static str,
    args: Vec<String>,
}

/// `copilot --acp --stdio` (Copilot CLI docs, ACP server page and command
/// reference, 2026-11-17; ACP is a public preview there). Model, effort and
/// permission are server flags, applied to every session it starts.
/// ⚠️ `--model`, the permission flags and `--allow-tool` are documented for
/// the CLI, not verified with `--acp`.
pub(crate) fn copilot(spec: &SessionSpec) -> Cli {
    let s = &spec.settings;
    let mut args = vec!["--acp".to_owned(), "--stdio".to_owned()];
    if let Some(model) = &s.model {
        args.push(format!("--model={model}"));
    }
    if let Some(effort) = &s.effort {
        args.push(format!("--effort={effort}"));
    }
    args.extend(copilot_flags(&s.permission).iter().map(|f| (*f).to_owned()));
    if !spec.mcp_servers.is_empty() {
        // The app's own tools (P10) run without asking: `SERVER-NAME` allows
        // every tool of that MCP server.
        let names: Vec<&str> = spec.mcp_servers.iter().map(|m| m.name.as_str()).collect();
        args.push(format!("--allow-tool={}", names.join(",")));
    }
    Cli {
        label: "Copilot CLI",
        program: "copilot",
        args,
    }
}

pub(crate) fn start(
    spec: &SessionSpec,
    cli: &Cli,
    path: Option<&OsString>,
) -> Result<BackendChannel, PortError> {
    let (session, first) = Session::new(spec, cli.label);
    let launch = Launch {
        path,
        program: cli.program,
        args: &cli.args,
        cwd: &spec.working_dir,
        label: cli.label,
    };
    rpc::start(&launch, session, first)
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
