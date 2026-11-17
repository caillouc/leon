//! Drives the coding CLIs (P2) and turns their output into
//! [`leon_core::ports::BackendEvent`]s.
//!
//! - Claude Code: `claude -p` with stream-json in and out.
//! - Codex: `codex app-server`, JSON-RPC over stdio.
//! - Copilot CLI: ACP (`copilot --acp`).
//!
//! Must not: depend on another adapter or on the UI; let a CLI-specific
//! field leave this crate.

mod acp;
mod claude;
mod codex;
mod doctor;
mod mcp;
mod options;
mod process;
mod rpc;
mod summary;
mod title;

use leon_core::ports::{
    BackendChannel, BackendOptions, Backends, PortError, SearchPath, SessionSpec, inherited_path,
};
use leon_domain::{BackendKind, DoctorSection};

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
            BackendKind::CopilotCli => acp::start(&spec, &acp::copilot(&spec), (self.path)()),
        }
    }

    fn options(&self, backend: BackendKind) -> BackendOptions {
        options::for_backend(backend)
    }

    fn title(&self, backend: BackendKind, message: &str) -> Result<String, PortError> {
        title::title((self.path)(), backend, message)
    }

    fn doctor(&self) -> Vec<DoctorSection> {
        BackendKind::ALL
            .into_iter()
            .map(|b| doctor::section((self.path)(), b))
            .collect()
    }
}
