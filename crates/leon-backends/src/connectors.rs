//! Each CLI's connectors for the Connectors view (TOOLS-1): its plugins, its
//! own MCP servers and its account's connectors, read with the CLI's own list
//! commands, as JSON wherever the CLI offers it. Checked live on 2026-11-18
//! with Claude Code 2.1.288, Codex 0.160.0 and Copilot CLI 1.0.113.
//! Timed that day: `claude mcp list` 4.2 s (no JSON output, connects to
//! every server), `copilot mcp list` 2.1 s, the others under 0.6 s, except a
//! first `codex plugin list` after a while: 115.7 s, then 0.3 s (⚠️ not
//! verified why). A CLI's two commands run at the same time.

mod parse;

use std::ffi::OsString;
use std::time::Duration;

use leon_domain::{BackendKind, CliConnectors, Connector, ConnectorKind};

use crate::process::{Output, RunError, clip, program, run, strings};

const TIMEOUT: Duration = Duration::from_secs(60);

/// Longest error kept from a CLI's stderr.
const ERROR: usize = 200;

/// Reads one list command's output; `None` when it is not what the CLI is
/// known to print.
type Parse = fn(&str) -> Option<Vec<Connector>>;

/// The list commands of `backend`'s CLI, with their readers.
fn commands(backend: BackendKind) -> [(&'static [&'static str], Parse); 2] {
    match backend {
        BackendKind::ClaudeCode => [
            (&["plugin", "list", "--json"], parse::claude_plugins),
            (&["mcp", "list"], |out| Some(parse::claude_servers(out))),
        ],
        BackendKind::Codex => [
            (&["plugin", "list", "--json"], parse::codex_plugins),
            // Servers that plugins bring are shown through their plugin.
            (
                &["--disable", "plugins", "mcp", "list", "--json"],
                parse::codex_servers,
            ),
        ],
        BackendKind::CopilotCli => [
            (&["plugin", "list", "--json"], parse::copilot_plugins),
            (&["mcp", "list", "--json"], parse::copilot_servers),
        ],
    }
}

/// `backend`'s connectors, read with the CLI found on `path`.
pub(crate) fn read(path: Option<&OsString>, backend: BackendKind) -> CliConnectors {
    let program = program(backend);
    read_with(backend, |args| run(path, program, &strings(args), TIMEOUT))
}

/// Why one list command gave no connectors.
enum Failure {
    NotInstalled,
    Problem(String),
}

/// `backend`'s connectors, each list command run by `run`, all at once. A
/// missing CLI gives only that problem; a failing command is named in the
/// problem and the other commands' connectors are kept.
fn read_with(
    backend: BackendKind,
    run: impl Fn(&[&str]) -> Result<Output, RunError> + Sync,
) -> CliConnectors {
    let program = program(backend);
    let commands = commands(backend);
    let ran: Vec<Result<Output, RunError>> = std::thread::scope(|scope| {
        let running: Vec<_> = commands
            .iter()
            .map(|(args, _)| scope.spawn(|| run(args)))
            .collect();
        running
            .into_iter()
            .map(|thread| {
                thread
                    .join()
                    .unwrap_or_else(|_| Err(RunError::Other("its thread stopped".to_owned())))
            })
            .collect()
    });
    let mut connectors = Vec::new();
    let mut problems = Vec::new();
    for ((args, parse), ran) in commands.into_iter().zip(ran) {
        match listed(program, args, ran, parse) {
            Ok(found) => connectors.extend(found),
            Err(Failure::NotInstalled) => {
                return CliConnectors {
                    backend,
                    connectors: Vec::new(),
                    problem: Some(format!(
                        "{} not installed: `{program}` was not found",
                        backend.display_name()
                    )),
                };
            }
            Err(Failure::Problem(problem)) => problems.push(problem),
        }
    }
    sort(&mut connectors);
    CliConnectors {
        backend,
        connectors,
        problem: (!problems.is_empty()).then(|| problems.join("; ")),
    }
}

fn listed(
    program: &str,
    args: &[&str],
    ran: Result<Output, RunError>,
    parse: Parse,
) -> Result<Vec<Connector>, Failure> {
    let command = format!("`{program} {}`", args.join(" "));
    let out = ran.map_err(|error| match error {
        RunError::NotFound => Failure::NotInstalled,
        RunError::TimedOut => {
            Failure::Problem(format!("{command} took more than {} s", TIMEOUT.as_secs()))
        }
        RunError::Other(message) => {
            Failure::Problem(format!("{command} could not start: {message}"))
        }
    })?;
    if !out.ok {
        let why = out
            .stderr
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("no message");
        return Err(Failure::Problem(format!(
            "{command} failed: {}",
            clip(why, ERROR)
        )));
    }
    parse(&out.stdout)
        .ok_or_else(|| Failure::Problem(format!("{command} printed a list Leon cannot read")))
}

/// By kind (account connectors, plugins, servers), then name.
fn sort(connectors: &mut [Connector]) {
    connectors.sort_by_cached_key(|c| (rank(c.kind), c.name.to_lowercase()));
}

fn rank(kind: ConnectorKind) -> u8 {
    match kind {
        ConnectorKind::Account => 0,
        ConnectorKind::Plugin => 1,
        ConnectorKind::Server => 2,
    }
}

#[cfg(test)]
mod tests;
