//! Drives the coding CLIs (P2) and turns their output into
//! [`leon_core::ports::BackendEvent`]s.
//!
//! - Claude Code: `claude -p` with stream-json in and out.
//! - Codex: `codex app-server`, JSON-RPC over stdio.
//! - Copilot CLI and Antigravity: ACP, not built yet (M8).
//!
//! Must not: depend on another adapter or on the UI; let a CLI-specific
//! field leave this crate.

mod claude;
mod codex;
mod mcp;
mod options;
mod process;
mod summary;

use leon_core::ports::{
    BackendChannel, BackendOptions, Backends, PortError, SearchPath, SessionSpec, inherited_path,
};
use leon_domain::BackendKind;

/// The real CLIs on this machine.
#[derive(Debug, Clone)]
pub struct Clis {
    path: SearchPath,
}

impl Clis {
    /// Runs the CLIs with the `PATH` that `path` gives.
    pub fn new(path: SearchPath) -> Self {
        Self { path }
    }
}

impl Default for Clis {
    /// Runs the CLIs with the inherited `PATH`.
    fn default() -> Self {
        Self::new(inherited_path)
    }
}

impl Backends for Clis {
    fn start(&self, spec: SessionSpec) -> Result<BackendChannel, PortError> {
        match spec.settings.backend {
            BackendKind::ClaudeCode => claude::start(&spec, (self.path)()),
            BackendKind::Codex => codex::start(&spec, (self.path)()),
            BackendKind::CopilotCli | BackendKind::Antigravity => Err(PortError::new(
                "starting a session",
                "this CLI is not supported yet",
            )),
        }
    }

    fn options(&self, backend: BackendKind) -> BackendOptions {
        options::for_backend(backend)
    }
}
