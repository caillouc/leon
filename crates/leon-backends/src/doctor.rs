//! Doctor checks for the coding CLIs: installed, version, logged in. Login
//! commands checked on 2026-11-17: `claude auth status` prints JSON with
//! `loggedIn` (2.1.288); `codex login status` prints "Logged in using …"
//! (0.160.0).

use std::ffi::OsString;
use std::time::Duration;

use leon_domain::{BackendKind, DoctorCheck, DoctorSection, Health, version_check};
use serde_json::Value;

use crate::process::{RunError, run};

const TIMEOUT: Duration = Duration::from_secs(15);

/// The CLI version Leon's adapter was checked with, if it was run.
fn checked_with(backend: BackendKind) -> Option<&'static str> {
    match backend {
        BackendKind::ClaudeCode => Some("2.1.288"),
        BackendKind::Codex => Some("0.160.0"),
        BackendKind::CopilotCli => None,
    }
}

fn program(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::ClaudeCode => "claude",
        BackendKind::Codex => "codex",
        BackendKind::CopilotCli => "copilot",
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

pub(crate) fn section(path: Option<&OsString>, backend: BackendKind) -> DoctorSection {
    let name = backend.display_name();
    let program = program(backend);
    let mut checks = Vec::new();
    match run(path, program, &strings(&["--version"]), TIMEOUT) {
        Err(RunError::NotFound) => {
            checks.push(
                DoctorCheck::new(
                    Health::Problem,
                    "Installed",
                    format!("`{program}` not found"),
                )
                .fix(format!(
                    "Install {name} so that `{program}` is on your PATH, then run the doctor again"
                )),
            );
            return DoctorSection {
                title: name.to_owned(),
                checks,
            };
        }
        Err(error) => checks.push(DoctorCheck::new(
            Health::Problem,
            "Installed",
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
            checks.push(match checked_with(backend) {
                Some(version) => version_check("Version", &found, version),
                None => DoctorCheck::new(
                    Health::Warning,
                    "Version",
                    format!("{found} (Leon was not tested with this CLI yet)"),
                ),
            });
        }
    }
    checks.push(login(path, backend));
    DoctorSection {
        title: name.to_owned(),
        checks,
    }
}

fn login(path: Option<&OsString>, backend: BackendKind) -> DoctorCheck {
    type Read = fn(bool, &str) -> Option<bool>;
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
        BackendKind::CopilotCli => {
            return DoctorCheck::new(
                Health::Warning,
                "Logged in",
                "not checked: Copilot CLI has no status command Leon knows",
            )
            .fix("Run `copilot` once in a terminal and log in if it asks");
        }
    };
    match run(path, program(backend), &strings(args), TIMEOUT) {
        Ok(out) => match read(out.ok, &format!("{}{}", out.stdout, out.stderr)) {
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
fn claude_logged_in(_ok: bool, out: &str) -> Option<bool> {
    serde_json::from_str::<Value>(out).ok()?["loggedIn"].as_bool()
}

/// `codex login status`: `Logged in using ChatGPT` (on stderr), or a
/// failure.
#[allow(
    clippy::unnecessary_wraps,
    reason = "same signature as the other login readers"
)]
fn codex_logged_in(ok: bool, out: &str) -> Option<bool> {
    Some(ok && out.trim_start().starts_with("Logged in"))
}

#[cfg(test)]
mod tests {
    use super::{claude_logged_in, codex_logged_in};

    #[test]
    fn login_answers_are_read() {
        assert_eq!(
            claude_logged_in(true, r#"{"loggedIn": false, "authMethod": "none"}"#),
            Some(false)
        );
        assert_eq!(claude_logged_in(true, r#"{"loggedIn": true}"#), Some(true));
        assert_eq!(claude_logged_in(true, "not json"), None);
        assert_eq!(
            codex_logged_in(true, "Logged in using ChatGPT\n"),
            Some(true)
        );
        assert_eq!(codex_logged_in(false, "Not logged in\n"), Some(false));
    }
}
