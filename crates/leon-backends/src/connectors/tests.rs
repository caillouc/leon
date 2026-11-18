use leon_domain::BackendKind;

use super::read_with;
use crate::process::{Output, RunError};

const CODEX_PLUGINS: &str = include_str!("../../fixtures/codex/0.160.0/plugin-list.json");
const CLAUDE_SERVERS: &str = include_str!("../../fixtures/claude/2.1.288/mcp-list.txt");

/// (CLI, what its plugin list gives, what its server list gives, connectors
/// kept, problem)
type Case = (
    BackendKind,
    Result<Output, RunError>,
    Result<Output, RunError>,
    usize,
    Option<&'static str>,
);

fn printed(stdout: &str) -> Output {
    Output {
        ok: true,
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

fn failed(stderr: &str) -> Output {
    Output {
        ok: false,
        stdout: String::new(),
        stderr: stderr.to_owned(),
    }
}

#[test]
fn problems_name_the_command_and_keep_the_other_lists() {
    let cases: [Case; 6] = [
        (
            BackendKind::Codex,
            Err(RunError::NotFound),
            Err(RunError::NotFound),
            0,
            Some("Codex not installed: `codex` was not found"),
        ),
        (
            BackendKind::Codex,
            Ok(printed(CODEX_PLUGINS)),
            Ok(failed("Error: config is invalid\n\n")),
            3,
            Some("`codex --disable plugins mcp list --json` failed: Error: config is invalid"),
        ),
        (
            BackendKind::ClaudeCode,
            Ok(printed("Installed plugins:")),
            Ok(printed(CLAUDE_SERVERS)),
            33,
            Some("`claude plugin list --json` printed a list Leon cannot read"),
        ),
        (
            BackendKind::ClaudeCode,
            Ok(printed("[]")),
            Err(RunError::TimedOut),
            0,
            Some("`claude mcp list` took more than 60 s"),
        ),
        (
            BackendKind::CopilotCli,
            Err(RunError::Other("permission denied".into())),
            Ok(failed("")),
            0,
            Some(
                "`copilot plugin list --json` could not start: permission denied; \
                 `copilot mcp list --json` failed: no message",
            ),
        ),
        (
            BackendKind::CopilotCli,
            Ok(printed("[]")),
            Ok(printed(r#"{"mcpServers": {}}"#)),
            0,
            None,
        ),
    ];
    for (backend, plugins, servers, kept, problem) in cases {
        let read = read_with(backend, |args| match args.first() {
            Some(&"plugin") => plugins.clone(),
            _ => servers.clone(),
        });
        assert_eq!(read.backend, backend);
        assert_eq!(read.connectors.len(), kept, "{backend:?}");
        assert_eq!(read.problem.as_deref(), problem, "{backend:?}");
    }
}

#[test]
fn connectors_are_sorted_by_kind_then_name() {
    let read = read_with(BackendKind::CopilotCli, |args| match args.first() {
        Some(&"plugin") => Ok(printed(
            r#"[{"name": "zeta", "marketplace": "", "version": "1", "enabled": true},
                {"name": "Alpha", "marketplace": "m", "version": "2", "enabled": false}]"#,
        )),
        _ => Ok(printed(
            r#"{"mcpServers": {"b-server": {"url": "https://example.invalid"},
                                         "a-server": {"command": "a"}}}"#,
        )),
    });
    let names: Vec<&str> = read.connectors.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Alpha@m", "zeta", "a-server", "b-server"]);
    assert_eq!(read.problem, None);

    let read = read_with(BackendKind::ClaudeCode, |args| match args.first() {
        Some(&"plugin") => Ok(printed(
            r#"[{"id": "kit@market", "version": "1", "enabled": true}]"#,
        )),
        _ => Ok(printed(
            "own: ./server - ✔ Connected\nclaude.ai monday.com: https://example.invalid - ✔ Connected\nclaude.ai Asana: https://example.invalid - ✔ Connected\n",
        )),
    });
    let names: Vec<&str> = read.connectors.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Asana", "monday.com", "kit@market", "own"]);
}
