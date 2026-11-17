//! The Tools view (TOOLS-1): Leon's shared MCP list, edited here and given
//! to every session (I14), and what each CLI has on its own.

use std::sync::Arc;

use leon_domain::{CliConnectors, McpServer};

use crate::app::Core;
use crate::repo::Internal;

impl Core {
    /// Reads the shared list again.
    pub(crate) fn load_shared(&mut self) {
        let shared = Arc::clone(&self.ports.shared_tools);
        self.state.shared_path = shared.path().to_owned();
        match tokio::task::block_in_place(|| shared.list()) {
            Ok(list) => {
                self.state.shared = Arc::new(list);
                self.state.shared_error = None;
            }
            Err(error) => self.state.shared_error = Some(error.to_string()),
        }
    }

    pub(crate) fn add_tool(&mut self, mut server: McpServer) {
        server.trusted = false;
        let name = server.name.clone();
        let shared = Arc::clone(&self.ports.shared_tools);
        match tokio::task::block_in_place(|| shared.add(&server)) {
            Ok(()) => self.flash(format!(
                "Added {name}: agent sessions started or resumed from now on get it."
            )),
            Err(error) => self.flash(format!("Could not add {name}: {error}")),
        }
        self.load_shared();
    }

    pub(crate) fn remove_tool(&mut self, name: &str) {
        let shared = Arc::clone(&self.ports.shared_tools);
        match tokio::task::block_in_place(|| shared.remove(name)) {
            Ok(()) => self.flash(format!("Removed {name}.")),
            Err(error) => self.flash(format!("Could not remove {name}: {error}")),
        }
        self.load_shared();
    }

    /// Asks each CLI for its own servers and plugins, in the background.
    pub(crate) fn refresh_tools(&mut self) {
        if self.state.tools_running {
            return;
        }
        self.state.tools_running = true;
        let backends = Arc::clone(&self.ports.backends);
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(Internal::Tools(backends.connectors()));
        });
    }

    pub(crate) fn tools_done(&mut self, connectors: Vec<CliConnectors>) {
        self.state.tools_running = false;
        self.state.tools = Some((self.now_ms(), Arc::new(connectors)));
    }

    /// The MCP servers a session gets: Leon's own tools, then the shared
    /// list (a shared server never replaces Leon's).
    pub(crate) fn session_servers(&self, own: Option<McpServer>) -> Vec<McpServer> {
        let mut servers: Vec<McpServer> = own.into_iter().collect();
        for s in self.state.shared.iter() {
            if !servers.iter().any(|o| o.name == s.name) {
                servers.push(s.clone());
            }
        }
        servers
    }
}
