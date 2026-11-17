use leon_domain::ConnectorKind::{Account, Plugin, Server};
use leon_domain::ConnectorState::{Failed, NeedsLogin, Off, On};
use leon_domain::{Connector, ConnectorKind, ConnectorState};

use super::{
    claude_plugins, claude_server, claude_servers, codex_plugins, codex_servers, connector,
    copilot_plugins, copilot_servers,
};

type Parse = fn(&str) -> Option<Vec<Connector>>;

/// (command, reader, recorded output, connectors by kind and state, one of
/// them in full)
type Recorded = (
    &'static str,
    Parse,
    &'static str,
    &'static [(ConnectorKind, ConnectorState, usize)],
    Connector,
);

#[test]
fn recorded_listings_are_read() {
    let cases: [Recorded; 6] = [
        (
            "claude plugin list --json",
            claude_plugins,
            include_str!("../../../fixtures/claude/2.1.288/plugin-list.json"),
            &[(Plugin, On, 1)],
            connector("team-kit@team-marketplace", Plugin, On, "547818e96a04"),
        ),
        (
            "claude mcp list",
            |out| Some(claude_servers(out)),
            include_str!("../../../fixtures/claude/2.1.288/mcp-list.txt"),
            &[
                (Account, On, 15),
                (Account, NeedsLogin, 20),
                (Account, Failed, 1),
            ],
            connector(
                "Mixpanel",
                Account,
                NeedsLogin,
                "https://mcp.mixpanel.com/mcp",
            ),
        ),
        (
            "codex plugin list --json",
            codex_plugins,
            include_str!("../../../fixtures/codex/0.160.0/plugin-list.json"),
            &[(Plugin, On, 16)],
            connector("slack@openai-curated-remote", Plugin, On, "0.1.8"),
        ),
        (
            "codex mcp list --json",
            codex_servers,
            include_str!("../../../fixtures/codex/0.160.0/mcp-list.json"),
            &[(Server, Off, 1), (Server, On, 2)],
            connector("codex_app", Server, Off, "/opt/tools/codex_app"),
        ),
        (
            "copilot plugin list --json",
            copilot_plugins,
            include_str!("../../../fixtures/copilot/1.0.113/plugin-list.json"),
            &[(Plugin, Off, 2)],
            connector("computer-use", Plugin, Off, "0.1.97"),
        ),
        (
            "copilot mcp list --json",
            copilot_servers,
            include_str!("../../../fixtures/copilot/1.0.113/mcp-list.json"),
            &[(Server, On, 2)],
            connector(
                "github-mcp-server",
                Server,
                On,
                "https://api.business.githubcopilot.com/mcp/readonly",
            ),
        ),
    ];
    for (command, parse, out, counts, sample) in cases {
        let found = parse(out).unwrap_or_else(|| panic!("{command}: not read"));
        let total: usize = counts.iter().map(|(_, _, n)| n).sum();
        assert_eq!(found.len(), total, "{command}: {found:#?}");
        for &(kind, state, n) in counts {
            let seen = found
                .iter()
                .filter(|c| c.kind == kind && c.state == state)
                .count();
            assert_eq!(seen, n, "{command}: {kind:?} {state:?}");
        }
        assert!(found.contains(&sample), "{command}: {sample:#?}");
    }
}

#[test]
fn claude_failure_keeps_the_error_short() {
    let found = claude_servers(include_str!(
        "../../../fixtures/claude/2.1.288/mcp-list.txt"
    ));
    let failed = found.iter().find(|c| c.state == Failed);
    let detail = failed.map_or("", |c| c.detail.as_str());
    assert!(
        detail.starts_with("Failed to connect — HTTP 410"),
        "{detail}"
    );
    assert!(detail.len() <= 200 + '…'.len_utf8(), "{detail}");
}

#[test]
fn claude_status_lines_are_read() {
    // Statuses not in the recording: Claude Code 2.1.288's own strings.
    let cases = [
        (
            "claude.ai Slack: https://mcp.slack.com/mcp - ✔ Connected",
            Some(("Slack", Account, On, "https://mcp.slack.com/mcp")),
        ),
        (
            "local: npx -y server - x - ✔ Connected",
            Some(("local", Server, On, "npx -y server - x")),
        ),
        (
            "docs: https://example.invalid/mcp - ! Connected · tools fetch failed",
            Some(("docs", Server, Failed, "Connected · tools fetch failed")),
        ),
        (
            "db: node db.js - - Not configured",
            Some(("db", Server, Failed, "Not configured")),
        ),
        (
            "repo: ./server - ✘ Connection error",
            Some(("repo", Server, Failed, "Connection error")),
        ),
        (
            "repo: ./server - ⏸ Pending approval (run `claude` to approve)",
            Some((
                "repo",
                Server,
                Off,
                "Pending approval (run `claude` to approve)",
            )),
        ),
        (
            "repo: ./server - ✘ Rejected (see disabledMcpjsonServers in settings)",
            Some((
                "repo",
                Server,
                Off,
                "Rejected (see disabledMcpjsonServers in settings)",
            )),
        ),
        (
            "repo: ./server - ⊘ Disabled for this project (re-enable via /mcp)",
            Some((
                "repo",
                Server,
                Off,
                "Disabled for this project (re-enable via /mcp)",
            )),
        ),
        ("Checking MCP server health…", None),
        ("", None),
        (
            "No MCP servers configured. Use `claude mcp add` to add a server.",
            None,
        ),
        ("odd: ./server - ? Some new status", None),
    ];
    for (line, expected) in cases {
        let expected =
            expected.map(|(name, kind, state, detail)| connector(name, kind, state, detail));
        assert_eq!(claude_server(line), expected, "{line}");
    }
}

#[test]
fn unexpected_json_is_not_a_crash() {
    // (reader, output, connectors read; None: not read)
    let cases: [(Parse, &str, Option<usize>); 14] = [
        (claude_plugins, "not json", None),
        (claude_plugins, "{}", None),
        (claude_plugins, r#"[{"enabled": true}]"#, Some(0)),
        (claude_plugins, r#"[{"id": "a@b", "new": [1]}]"#, Some(1)),
        (codex_plugins, "[]", None),
        (codex_plugins, r#"{"installed": [], "more": 1}"#, Some(0)),
        (
            codex_plugins,
            r#"{"installed": [{"pluginId": 3}]}"#,
            Some(0),
        ),
        (copilot_plugins, r#"{"name": "x"}"#, None),
        (
            copilot_plugins,
            r#"[{"name": "x", "marketplace": "m"}]"#,
            Some(1),
        ),
        (codex_servers, "{}", None),
        (codex_servers, r#"[{"name": "x"}]"#, Some(1)),
        (copilot_servers, "[]", None),
        (copilot_servers, r#"{"mcpServers": {}}"#, Some(0)),
        (
            copilot_servers,
            r#"{"mcpServers": {"x": {"type": "new"}}}"#,
            Some(1),
        ),
    ];
    for (parse, out, expected) in cases {
        assert_eq!(parse(out).map(|found| found.len()), expected, "{out}");
    }
}

#[test]
fn states_follow_the_cli_fields() {
    let cases: [(Parse, &str, Connector); 5] = [
        (
            codex_servers,
            r#"[{"name": "x", "enabled": true, "auth_status": "not_logged_in",
                "transport": {"url": "https://example.invalid/mcp"}}]"#,
            connector("x", Server, NeedsLogin, "https://example.invalid/mcp"),
        ),
        (
            codex_servers,
            r#"[{"name": "x", "enabled": false, "auth_status": "not_logged_in"}]"#,
            connector("x", Server, Off, ""),
        ),
        (
            claude_plugins,
            r#"[{"id": "a@b", "version": "1.0.0", "enabled": false}]"#,
            connector("a@b", Plugin, Off, "1.0.0"),
        ),
        (
            copilot_plugins,
            r#"[{"name": "a", "marketplace": "b", "version": "2", "enabled": true}]"#,
            connector("a@b", Plugin, On, "2"),
        ),
        (
            copilot_servers,
            r#"{"mcpServers": {"x": {"type": "stdio", "command": "x-mcp", "enabled": false}}}"#,
            connector("x", Server, Off, "x-mcp"),
        ),
    ];
    for (parse, out, expected) in cases {
        assert_eq!(parse(out), Some(vec![expected]), "{out}");
    }
}
