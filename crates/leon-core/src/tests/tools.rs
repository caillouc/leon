//! Leon's shared MCP list and the Tools view (TOOLS-1, I14).

use std::sync::Mutex;

use leon_domain::McpServer;

use super::*;
use crate::ports::SharedTools;

#[derive(Default)]
pub(super) struct FakeShared {
    servers: Mutex<Vec<McpServer>>,
}

impl SharedTools for FakeShared {
    fn path(&self) -> &Path {
        Path::new("/config/leon/mcp.json")
    }
    fn list(&self) -> Result<Vec<McpServer>, PortError> {
        Ok(self.servers.lock().unwrap().clone())
    }
    fn add(&self, server: &McpServer) -> Result<(), PortError> {
        let mut servers = self.servers.lock().unwrap();
        servers.retain(|s| s.name != server.name);
        servers.push(server.clone());
        Ok(())
    }
    fn remove(&self, name: &str) -> Result<(), PortError> {
        self.servers.lock().unwrap().retain(|s| s.name != name);
        Ok(())
    }
}

fn server(name: &str) -> McpServer {
    McpServer {
        name: name.into(),
        command: PathBuf::from("npx"),
        args: vec![format!("{name}-mcp")],
        env: Vec::new(),
        trusted: true,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shared_server_reaches_new_sessions_untrusted() {
    let h = harness();
    until(&h.core, "loaded", |s| s.loaded).await;
    h.core.send(Intent::AddTool(server("metabase")));
    h.core.send(Intent::AddTool(server("leon")));
    let s = until(&h.core, "listed", |s| s.shared_tools.len() == 2).await;
    assert!(s.shared_tools.iter().all(|m| !m.trusted));
    new_conversation(&h, BackendKind::Codex).await;
    let spec = h.backends.started.lock().unwrap()[0].spec.clone();
    let names: Vec<(&str, bool)> = spec
        .mcp_servers
        .iter()
        .map(|m| (m.name.as_str(), m.trusted))
        .collect();
    // Leon's own server keeps its name; the shared one named "leon" is left out.
    assert_eq!(names, [("leon", true), ("metabase", false)]);

    h.core.send(Intent::RemoveTool("metabase".into()));
    until(&h.core, "removed", |s| {
        s.shared_tools.iter().all(|m| m.name != "metabase")
    })
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cli_listing_arrives_in_the_background() {
    let h = harness();
    until(&h.core, "loaded", |s| s.loaded).await;
    h.core.send(Intent::RefreshTools);
    let s = until(&h.core, "tools", |s| s.tools.is_some()).await;
    assert_eq!(s.tools.as_ref().unwrap().1[0].checks[0].name, "node_repl");
    assert!(!s.tools_running);
}
