use leon_domain::{BackendKind, Health};

use super::{claude_logged_in, codex_logged_in, section_with};
use crate::process::{Output, RunError};

const COPILOT_LOGGED_IN: &str = include_str!("../../fixtures/copilot/1.0.113/mcp-list.json");
const COPILOT_LOGGED_OUT: &str =
    include_str!("../../fixtures/copilot/1.0.113/mcp-list-logged-out.json");

fn said(ok: bool, stdout: &str, stderr: &str) -> Output {
    Output {
        ok,
        stdout: stdout.to_owned(),
        stderr: stderr.to_owned(),
    }
}

#[test]
fn login_answers_are_read() {
    let cases = [
        (
            claude_logged_in(&said(
                true,
                r#"{"loggedIn": false, "authMethod": "none"}"#,
                "",
            )),
            Some(false),
        ),
        (
            claude_logged_in(&said(true, r#"{"loggedIn": true}"#, "")),
            Some(true),
        ),
        (claude_logged_in(&said(true, "not json", "")), None),
        (
            codex_logged_in(&said(true, "", "Logged in using ChatGPT\n")),
            Some(true),
        ),
        (
            codex_logged_in(&said(false, "", "Not logged in\n")),
            Some(false),
        ),
    ];
    for (ix, (read, expected)) in cases.into_iter().enumerate() {
        assert_eq!(read, expected, "case {ix}");
    }
}

/// (what `copilot mcp list --json` gives, health of "Logged in", detail)
type CopilotCase = (Result<Output, RunError>, Health, &'static str);

#[test]
fn copilot_login_is_read_from_its_server_list() {
    let cases: [CopilotCase; 5] = [
        (Ok(said(true, COPILOT_LOGGED_IN, "")), Health::Ok, "yes"),
        (
            Ok(said(true, COPILOT_LOGGED_OUT, "")),
            Health::Problem,
            "no",
        ),
        (
            Ok(said(
                true,
                COPILOT_LOGGED_OUT,
                "Warning: timed out checking for the built-in GitHub MCP server, so it is \
                 not shown here.\n",
            )),
            Health::Warning,
            "could not tell from the CLI's answer",
        ),
        (
            Ok(said(
                true,
                r#"{"mcpServers": {"github-mcp-server": {"source": "user"}}}"#,
                "",
            )),
            Health::Warning,
            "could not tell from the CLI's answer",
        ),
        (
            Err(RunError::TimedOut),
            Health::Warning,
            "could not ask: TimedOut",
        ),
    ];
    for (ix, (listed, health, detail)) in cases.into_iter().enumerate() {
        let section = section_with(BackendKind::CopilotCli, |args| match args {
            ["--version"] => Ok(said(
                true,
                "GitHub Copilot CLI 1.0.114.\nRun 'copilot update' to check for updates.\n",
                "",
            )),
            ["mcp", "list", "--json"] => listed.clone(),
            _ => Err(RunError::Other(format!("unexpected {args:?}"))),
        });
        let login = section.checks.iter().find(|c| c.name == "Logged in");
        let got = login.map(|c| (c.health, c.detail.as_str()));
        assert_eq!(got, Some((health, detail)), "case {ix}");
        // Leon ran Copilot CLI 1.0.114: no "not tested" warning.
        let version = section.checks.iter().find(|c| c.name == "Version");
        assert_eq!(version.map(|c| c.health), Some(Health::Ok), "case {ix}");
    }
}
