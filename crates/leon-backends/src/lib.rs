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
mod bridge;
mod claude;
mod codex;
mod connectors;
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
use leon_domain::{BackendKind, CliConnectors, DoctorSection};

/// The real CLIs on this machine.
#[derive(Debug, Clone)]
pub struct Clis {
    path: SearchPath,
    /// Where Leon copies its generic skills for the CLIs (EXT-5); none in
    /// tests.
    skills: Option<std::path::PathBuf>,
}

impl Clis {
    /// Runs the CLIs with the `PATH` that `path` gives.
    pub fn new(path: SearchPath) -> Self {
        Self { path, skills: None }
    }

    /// Hands Leon's generic skills to the CLIs through copies in `folder`,
    /// a folder of Leon's own (EXT-5).
    #[must_use]
    pub fn with_skills_folder(mut self, folder: std::path::PathBuf) -> Self {
        self.skills = Some(folder);
        self
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
        let backend = spec.settings.backend;
        let skills = self
            .skills
            .as_deref()
            .and_then(|root| bridge::prepare(root, backend, &spec.skills));
        let skills = skills.as_deref();
        match backend {
            BackendKind::ClaudeCode => claude::start(&spec, (self.path)(), skills),
            BackendKind::Codex => codex::start(&spec, (self.path)(), skills),
            BackendKind::CopilotCli => {
                acp::start(&spec, &acp::copilot(&spec, skills), (self.path)())
            }
        }
    }

    fn options(&self, backend: BackendKind) -> BackendOptions {
        options::for_backend(backend)
    }

    fn title(&self, backend: BackendKind, message: &str) -> Result<String, PortError> {
        title::title((self.path)(), backend, message)
    }

    fn connectors(&self) -> Vec<CliConnectors> {
        BackendKind::ALL
            .into_iter()
            .map(|b| connectors::read((self.path)(), b))
            .collect()
    }

    fn doctor(&self) -> Vec<DoctorSection> {
        BackendKind::ALL
            .into_iter()
            .map(|b| doctor::section((self.path)(), b))
            .collect()
    }
}
