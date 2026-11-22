//! Doctor checks for the coding CLIs: installed, version, logged in. Login
//! commands checked on 2026-11-17: `claude auth status` prints JSON with
//! `loggedIn` (2.1.288); `codex login status` prints "Logged in using …"
//! (0.160.0). Copilot CLI has none (1.0.113, checked on 2026-11-18): its MCP
//! listing tells instead. Each CLI is optional: one not installed is only a
//! fact, unless none is.

use std::ffi::OsString;
use std::time::Duration;

use leon_domain::{BackendKind, DoctorCheck, DoctorSection, Health, version_check};
use serde_json::Value;

use crate::process::{Output, RunError, program, run, strings};

const TIMEOUT: Duration = Duration::from_secs(15);

/// The CLI version Leon's adapter was checked with.
fn checked_with(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::ClaudeCode => "2.1.288",
        BackendKind::Codex => "0.160.0",
        // Live tests (pong, a model without effort, Leon's own tools) pass on
        // 1.0.114, 2026-11-18.
        BackendKind::CopilotCli => "1.0.114",
    }
}

const INSTALLED: &str = "Installed";
const NOT_INSTALLED: &str = "no (optional)";

/// Leon needs one coding CLI: when none is installed, each "not installed"
/// is a problem.
pub(crate) fn require_one(sections: &mut [DoctorSection]) {
    let missing = |c: &DoctorCheck| c.name == INSTALLED && c.detail == NOT_INSTALLED;
    if sections.iter().all(|s| s.checks.iter().any(missing)) {
        for check in sections.iter_mut().flat_map(|s| &mut s.checks) {
            if missing(check) {
                check.health = Health::Problem;
                "no: Leon needs at least one coding CLI".clone_into(&mut check.detail);
            }
        }
    }
}

pub(crate) fn section(path: Option<&OsString>, backend: BackendKind) -> DoctorSection {
    let program = program(backend);
    section_with(backend, |args| run(path, program, &strings(args), TIMEOUT))
}

/// `backend`'s section, each command run by `run`.
fn section_with(
    backend: BackendKind,
    run: impl Fn(&[&str]) -> Result<Output, RunError>,
) -> DoctorSection {
    let name = backend.display_name();
    let program = program(backend);
    let mut checks = Vec::new();
    match run(&["--version"]) {
        Err(RunError::NotFound) => {
            checks.push(
                DoctorCheck::new(Health::Info, INSTALLED, NOT_INSTALLED).fix(format!(
                    "To use it, install {name} so that `{program}` is on your PATH"
                )),
            );
            return DoctorSection {
                title: name.to_owned(),
                checks,
            };
        }
        Err(error) => checks.push(DoctorCheck::new(
            Health::Problem,
            INSTALLED,
            format!("`{program} --version`: {error:?}"),
        )),
        Ok(out) => {
            let found = out
                .stdout
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned();
            checks.push(version_check("Version", &found, checked_with(backend)));
        }
    }
    checks.push(login(backend, &run));
    DoctorSection {
        title: name.to_owned(),
        checks,
    }
}

fn login(backend: BackendKind, run: impl Fn(&[&str]) -> Result<Output, RunError>) -> DoctorCheck {
    type Read = fn(&Output) -> Option<bool>;
    let (args, read, fix): (&[&str], Read, &str) = match backend {
        BackendKind::ClaudeCode => (
            &["auth", "status", "--json"],
            claude_logged_in,
            "Run `claude auth login` in a terminal",
        ),
        BackendKind::Codex => (
            &["login", "status"],
            codex_logged_in,
            "Run `codex login` in a terminal",
        ),
        BackendKind::CopilotCli => (
            &["mcp", "list", "--json"],
            copilot_logged_in,
            "Run `copilot login` in a terminal",
        ),
    };
    match run(args) {
        Ok(out) => match read(&out) {
            Some(true) => DoctorCheck::new(Health::Ok, "Logged in", "yes"),
            Some(false) => DoctorCheck::new(Health::Problem, "Logged in", "no").fix(fix),
            None => DoctorCheck::new(
                Health::Warning,
                "Logged in",
                "could not tell from the CLI's answer",
            )
            .fix(fix),
        },
        Err(error) => DoctorCheck::new(
            Health::Warning,
            "Logged in",
            format!("could not ask: {error:?}"),
        ),
    }
}

/// `claude auth status --json`: `{"loggedIn": bool, …}`.
fn claude_logged_in(out: &Output) -> Option<bool> {
    serde_json::from_str::<Value>(&out.stdout).ok()?["loggedIn"].as_bool()
}

/// `codex login status`: `Logged in using ChatGPT` (on stderr), or a
/// failure.
#[allow(
    clippy::unnecessary_wraps,
    reason = "same signature as the other login readers"
)]
fn codex_logged_in(out: &Output) -> Option<bool> {
    let said = format!("{}{}", out.stdout, out.stderr);
    Some(out.ok && said.trim_start().starts_with("Logged in"))
}

/// What Copilot CLI prints on stderr when it could not check its built-in
/// GitHub server ("timed out checking for …" or "could not check for …").
const COPILOT_UNCHECKED: &str = "the built-in GitHub MCP server, so it is not shown here";

/// `copilot mcp list --json`: Copilot CLI has no login status command, but it
/// lists its built-in `github-mcp-server` (`"source": "builtin"`) only once
/// logged in. Checked on 2026-11-18 with 1.0.113: listed when logged in,
/// absent from an empty home with no token. A server of the user's under that
/// name hides the built-in one, so it tells nothing.
fn copilot_logged_in(out: &Output) -> Option<bool> {
    if !out.ok || out.stderr.contains(COPILOT_UNCHECKED) {
        return None;
    }
    let list = serde_json::from_str::<Value>(&out.stdout).ok()?;
    match list["mcpServers"].as_object()?.get("github-mcp-server") {
        None => Some(false),
        Some(server) => (server["source"].as_str() == Some("builtin")).then_some(true),
    }
}

#[cfg(test)]
mod tests;
