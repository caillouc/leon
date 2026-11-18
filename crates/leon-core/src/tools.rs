//! The Tools view (TOOLS-1): Leon's shared MCP list, edited here and given
//! to every conversation (I14), and what each CLI has on its own.

use std::sync::Arc;

use leon_domain::{BackendKind, CliConnectors, McpServer};

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
                "Added {name}: conversations started or resumed from now on get it."
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

    /// Asks every CLI for its own servers and plugins, all at once in the
    /// background: each CLI's answer is shown as it arrives.
    pub(crate) fn refresh_tools(&mut self) {
        if !self.state.tools_pending.is_empty() {
            return;
        }
        self.state.tools_pending = BackendKind::ALL.to_vec();
        for backend in BackendKind::ALL {
            let backends = Arc::clone(&self.ports.backends);
            let tx = self.internal_tx.clone();
            tokio::task::spawn_blocking(move || {
                let _ = tx.send(Internal::Tools(backends.connectors(backend)));
            });
        }
    }

    /// One CLI's answer, in place of its previous one; the CLIs stay in
    /// [`BackendKind::ALL`] order.
    pub(crate) fn tools_done(&mut self, cli: CliConnectors) {
        self.state.tools_pending.retain(|b| *b != cli.backend);
        let mut clis: Vec<CliConnectors> = self
            .state
            .tools
            .as_ref()
            .map_or_else(Vec::new, |(_, clis)| clis.as_ref().clone());
        clis.retain(|c| c.backend != cli.backend);
        clis.push(cli);
        clis.sort_by_key(|c| BackendKind::ALL.iter().position(|b| *b == c.backend));
        self.state.tools = Some((self.now_ms(), Arc::new(clis)));
    }

    /// The MCP servers a conversation gets: Leon's own tools, then the shared
    /// list (a shared server never replaces Leon's).
    pub(crate) fn conversation_servers(&self, own: Option<McpServer>) -> Vec<McpServer> {
        let mut servers: Vec<McpServer> = own.into_iter().collect();
        for s in self.state.shared.iter() {
            if !servers.iter().any(|o| o.name == s.name) {
                servers.push(s.clone());
            }
        }
        servers
    }
}
