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
mod claude_ask;
mod codex;
mod conn;
mod connectors;
mod doctor;
mod holder;
mod images;
mod limits;
mod mcp;
mod options;
mod plan;
mod process;
mod rpc;
mod sessions;
mod summary;
mod title;
mod told;

pub use holder::hold;
use leon_core::ports::{
    Attached, BackendChannel, BackendOptions, Backends, PortError, SearchPath, SessionSpec,
    inherited_path,
};
use leon_domain::{BackendKind, CliConnectors, DoctorCheck, DoctorSection, Health};

/// The real CLIs on this machine.
#[derive(Debug, Clone)]
pub struct Clis {
    path: SearchPath,
    /// Where Leon copies its generic skills for the CLIs (EXT-5); none in
    /// tests.
    skills: Option<std::path::PathBuf>,
    /// The holders the CLIs run under, so they outlive Leon (I24); none in
    /// tests: the CLIs are Leon's own children there.
    holders: Option<conn::Holders>,
    /// The user's home, where the CLIs keep their sessions: a deleted
    /// conversation's goes with it (NAV-8); none in tests.
    home: Option<std::path::PathBuf>,
    /// Where the images of messages are kept, a folder per conversation
    /// (CONV-18, I27); none in tests.
    images: Option<std::path::PathBuf>,
}

impl Clis {
    /// Runs the CLIs with the `PATH` that `path` gives.
    pub fn new(path: SearchPath) -> Self {
        Self {
            path,
            skills: None,
            holders: None,
            home: None,
            images: None,
        }
    }

    /// Keeps the images of messages in a folder of `root` per conversation
    /// (CONV-18, I27).
    #[must_use]
    pub fn with_images(mut self, root: std::path::PathBuf) -> Self {
        self.images = Some(root);
        self
    }

    /// Hands Leon's generic skills to the CLIs through copies in `folder`,
    /// a folder of Leon's own (EXT-5).
    #[must_use]
    pub fn with_skills_folder(mut self, folder: std::path::PathBuf) -> Self {
        self.skills = Some(folder);
        self
    }

    /// Removes a deleted conversation's session from the CLI's files under
    /// `home`, where the CLI keeps them in files alone (NAV-8).
    #[must_use]
    pub fn with_home(mut self, home: std::path::PathBuf) -> Self {
        self.home = Some(home);
        self
    }

    /// Runs each CLI under a holder, `exe hold …`, with its files in a
    /// folder of `runs` (I24): the CLIs keep working when Leon quits.
    #[must_use]
    pub fn with_holders(mut self, exe: std::path::PathBuf, runs: std::path::PathBuf) -> Self {
        self.holders = Some(conn::Holders { exe, runs });
        self
    }

    /// Leon's skills folder for `spec`'s CLI, prepared.
    fn skills_for(&self, spec: &SessionSpec) -> Option<std::path::PathBuf> {
        self.skills
            .as_deref()
            .and_then(|root| bridge::prepare(root, spec.settings.backend, &spec.skills))
    }
}

/// `channel` with Leon's marks kept in its holder's folder `dir`: a turn
/// starts with a message, ends with its end (I24).
fn marked(channel: BackendChannel, dir: std::path::PathBuf) -> BackendChannel {
    use leon_core::ports::{BackendCommand, BackendEvent};
    let (commands, mut asked) = tokio::sync::mpsc::unbounded_channel();
    let (told, events) = tokio::sync::mpsc::unbounded_channel();
    let BackendChannel {
        commands: inner,
        events: mut heard,
    } = channel;
    let started = dir.clone();
    tokio::spawn(async move {
        while let Some(command) = asked.recv().await {
            if matches!(command, BackendCommand::Send(_)) {
                conn::turn_started(&started);
            }
            if inner.send(command).is_err() {
                return;
            }
        }
    });
    tokio::spawn(async move {
        while let Some(event) = heard.recv().await {
            if matches!(
                event,
                BackendEvent::TurnEnded { .. } | BackendEvent::Failed(_)
            ) {
                conn::turn_ended(&dir);
            }
            if told.send(event).is_err() {
                return;
            }
        }
    });
    BackendChannel { commands, events }
}

impl Default for Clis {
    /// Runs the CLIs with the inherited `PATH`.
    fn default() -> Self {
        Self::new(inherited_path)
    }
}

impl Backends for Clis {
    fn start(&self, spec: SessionSpec) -> Result<BackendChannel, PortError> {
        let skills = self.skills_for(&spec);
        let skills = skills.as_deref();
        let holders = self.holders.as_ref();
        let channel = match spec.settings.backend {
            BackendKind::ClaudeCode => claude::start(&spec, (self.path)(), skills, holders),
            BackendKind::Codex => codex::start(&spec, (self.path)(), skills, holders),
            BackendKind::CopilotCli => {
                acp::start(&spec, &acp::copilot(&spec, skills), (self.path)(), holders)
            }
        }?;
        Ok(match holders {
            Some(holders) => marked(channel, holders.dir(&spec.conversation.0.to_string())),
            None => channel,
        })
    }

    fn held(&self) -> Vec<leon_domain::ConversationId> {
        let Some(holders) = self.holders.as_ref() else {
            return Vec::new();
        };
        std::fs::read_dir(&holders.runs).map_or_default(|dir| {
            dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
                .map(leon_domain::ConversationId)
                .collect()
        })
    }

    fn attach(&self, spec: SessionSpec) -> Option<Attached> {
        let holders = self.holders.as_ref()?;
        let key = spec.conversation.0.to_string();
        let (opening, busy) = conn::find(holders, &key)?;
        let skills = self.skills_for(&spec);
        let skills = skills.as_deref();
        let channel = match spec.settings.backend {
            BackendKind::ClaudeCode => claude::session(opening),
            BackendKind::Codex => codex::attach(opening, &spec, skills),
            BackendKind::CopilotCli => acp::attach(opening, &spec, &acp::copilot(&spec, skills)),
        };
        Some(Attached {
            channel: marked(channel, holders.dir(&key)),
            busy,
        })
    }

    fn options(&self, backend: BackendKind) -> BackendOptions {
        options::for_backend(backend)
    }

    fn discard(
        &self,
        id: leon_domain::ConversationId,
        session: Option<leon_core::ports::CliSession>,
    ) {
        if let Some(holders) = self.holders.as_ref() {
            conn::discard(holders, &id.0.to_string());
        }
        if let Some(root) = self.images.as_deref()
            && let Err(error) = std::fs::remove_dir_all(images::folder(root, id))
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, conversation = id.0, "removing a conversation's images");
        }
        // Codex and Copilot CLI index their sessions in databases of their
        // own too: theirs stay (sessions.rs).
        if let (Some(home), Some((BackendKind::ClaudeCode, session))) =
            (self.home.as_deref(), session)
        {
            sessions::discard_claude(&home.join(".claude").join("projects"), &session);
        }
    }

    fn plan_usage(&self, backend: BackendKind) -> Option<leon_core::ports::PlanSaid> {
        match backend {
            BackendKind::Codex => plan::codex((self.path)()),
            BackendKind::ClaudeCode | BackendKind::CopilotCli => None,
        }
    }

    fn save_image(
        &self,
        id: leon_domain::ConversationId,
        image: &leon_core::ports::Image,
    ) -> Result<std::path::PathBuf, PortError> {
        let root = self
            .images
            .as_deref()
            .ok_or_else(|| PortError::new("saving an image", "no folder for images"))?;
        images::save(root, id, image)
    }

    fn title(&self, backend: BackendKind, message: &str) -> Result<String, PortError> {
        title::title((self.path)(), backend, message)
    }

    fn connectors(&self, backend: BackendKind) -> CliConnectors {
        connectors::read((self.path)(), backend)
    }

    /// The CLIs are checked at the same time.
    fn doctor(&self) -> Vec<DoctorSection> {
        let mut sections = std::thread::scope(|scope| {
            let checking: Vec<_> = BackendKind::ALL
                .into_iter()
                .map(|b| (b, scope.spawn(move || doctor::section((self.path)(), b))))
                .collect();
            checking
                .into_iter()
                .map(|(b, thread)| {
                    thread.join().unwrap_or_else(|_| DoctorSection {
                        title: b.display_name().to_owned(),
                        checks: vec![DoctorCheck::new(
                            Health::Problem,
                            "Checked",
                            "the check stopped unexpectedly",
                        )],
                    })
                })
                .collect::<Vec<_>>()
        });
        doctor::require_one(&mut sections);
        sections
    }
}
