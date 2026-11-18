use leon_domain::ConnectorKind::{Account, Plugin, Server};
use leon_domain::ConnectorState::{Failed, NeedsLogin, Off, On};
use leon_domain::{Connector, ConnectorKind, ConnectorState};

use super::{
    claude_plugins, claude_server, claude_servers, codex_plugins, codex_servers, connector,
    copilot_plugins, copilot_servers,
};

type Parse = fn(&str) -> Option<Vec<Connector>>;

/// (command, reader, recorded output, connectors kept by kind and state, one
/// of them in full)
type Recorded = (
    &'static str,
    Parse,
    &'static str,
    &'static [(ConnectorKind, ConnectorState, usize)],
    Option<Connector>,
);

#[test]
fn claude_and_codex_listings_are_read() {
    let cases: [Recorded; 5] = [
        (
            "claude plugin list --json",
            claude_plugins,
            include_str!("../../../fixtures/claude/2.1.288/plugin-list.json"),
            &[(Plugin, On, 1)],
            Some(connector(
                "team-kit@team-marketplace",
                Plugin,
                On,
                "547818e96a04",
            )),
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
            Some(connector(
                "Mixpanel",
                Account,
                NeedsLogin,
                "https://mcp.mixpanel.com/mcp",
            )),
        ),
        (
            // 16 installed: 3 the user picked, 3 installed for every account,
            // 10 from OpenAI's bundled marketplaces.
            "codex plugin list --json",
            codex_plugins,
            include_str!("../../../fixtures/codex/0.160.0/plugin-list.json"),
            &[(Plugin, On, 3)],
            Some(connector(
                "slack@openai-curated-remote",
                Plugin,
                On,
                "0.1.8",
            )),
        ),
        (
            "codex mcp list --json",
            codex_servers,
            include_str!("../../../fixtures/codex/0.160.0/mcp-list.json"),
            &[(Server, Off, 1), (Server, On, 2)],
            Some(connector("codex_app", Server, Off, "/opt/tools/codex_app")),
        ),
        (
            // Without the bundled plugins' servers (`codex_app`, `cua_repl`).
            "codex --disable plugins mcp list --json",
            codex_servers,
            include_str!("../../../fixtures/codex/0.160.0/mcp-list-no-plugins.json"),
            &[(Server, On, 1)],
            Some(connector(
                "node_repl",
                Server,
                On,
                "/Applications/ChatGPT.app/Contents/Resources/cua_node/bin/node_repl",
            )),
        ),
    ];
    check(&cases);
}

#[test]
fn copilot_listings_are_read() {
    let cases: [Recorded; 3] = [
        (
            // Only Copilot's own `computer-use` and `browser-use`.
            "copilot plugin list --json",
            copilot_plugins,
            include_str!("../../../fixtures/copilot/1.0.113/plugin-list.json"),
            &[],
            None,
        ),
        (
            // Only Copilot's own `computer-use` and `github-mcp-server`.
            "copilot mcp list --json",
            copilot_servers,
            include_str!("../../../fixtures/copilot/1.0.113/mcp-list.json"),
            &[],
            None,
        ),
        (
            // An empty home where two servers were added, next to Copilot's
            // own `computer-use`.
            "copilot mcp list --json, logged out",
            copilot_servers,
            include_str!("../../../fixtures/copilot/1.0.113/mcp-list-logged-out.json"),
            &[(Server, On, 2)],
            Some(connector(
                "tracker",
                Server,
                On,
                "https://mcp.example.invalid/mcp",
            )),
        ),
    ];
    check(&cases);
}

fn check(cases: &[Recorded]) {
    for (command, parse, out, counts, sample) in cases {
        let found = parse(out).unwrap_or_else(|| panic!("{command}: not read"));
        let total: usize = counts.iter().map(|(_, _, n)| n).sum();
        assert_eq!(found.len(), total, "{command}: {found:#?}");
        for &(kind, state, n) in *counts {
            let seen = found
                .iter()
                .filter(|c| c.kind == kind && c.state == state)
                .count();
            assert_eq!(seen, n, "{command}: {kind:?} {state:?}");
        }
        if let Some(sample) = sample {
            assert!(found.contains(sample), "{command}: {sample:#?}");
        }
    }
}

#[test]
fn what_a_cli_ships_or_turns_on_itself_is_left_out() {
    // (reader, output, names kept)
    let cases: [(Parse, &str, &[&str]); 5] = [
        (
            codex_plugins,
            r#"{"installed": [
                {"pluginId": "kit@team", "marketplaceName": "team", "installPolicy": "AVAILABLE"},
                {"pluginId": "pets@openai-curated-remote",
                 "marketplaceName": "openai-curated-remote",
                 "installPolicy": "INSTALLED_BY_DEFAULT"},
                {"pluginId": "pdf@openai-primary-runtime",
                 "marketplaceName": "openai-primary-runtime"},
                {"pluginId": "browser@openai-bundled", "marketplaceName": "openai-bundled"},
                {"pluginId": "new@openai-bundled-alpha",
                 "marketplaceName": "openai-bundled-alpha"}
            ]}"#,
            &["kit@team"],
        ),
        (
            codex_plugins,
            r#"{"installed": [{"pluginId": "github@openai-curated",
                               "marketplaceName": "openai-curated"}]}"#,
            &["github@openai-curated"],
        ),
        (
            copilot_plugins,
            r#"[{"name": "kit", "marketplace": "team", "source": "marketplace"},
                {"name": "computer-use", "marketplace": "", "source": "builtin"}]"#,
            &["kit@team"],
        ),
        (
            copilot_servers,
            r#"{"mcpServers": {"own": {"source": "user"}, "repo": {"source": "workspace"},
                               "kit": {"source": "plugin"},
                               "github-mcp-server": {"source": "builtin"}}}"#,
            &["kit", "own", "repo"],
        ),
        (
            copilot_servers,
            r#"{"mcpServers": {"no-source": {"command": "x"}}}"#,
            &["no-source"],
        ),
    ];
    for (parse, out, kept) in cases {
        let found = parse(out).unwrap_or_default();
        // Sorted: the order of JSON keys depends on serde_json's features.
        let mut names: Vec<&str> = found.iter().map(|c| c.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, kept, "{out}");
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
