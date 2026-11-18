//! Core behaviour with fake storage, CLIs, git and GitHub.

mod agents;
mod repo;
mod rewind;
mod tools;
mod turns;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, Conversation, ConversationId, Entry, Project,
    ProjectId, SessionSettings, Settings, Status, TimedEntry,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::ports::{
    AgentDefinition, BackendChannel, BackendCommand, BackendEvent, BackendOptions, Backends,
    Catalog, NewSession, PortError, SessionSpec, Storage,
};
use crate::{CoreHandle, Intent, Ports, Snapshot, start};

#[derive(Default)]
struct FakeStorage {
    projects: Mutex<Vec<Project>>,
    conversations: Mutex<Vec<Conversation>>,
    sessions: Mutex<Vec<AgentSession>>,
    entries: Mutex<HashMap<AgentSessionId, Vec<TimedEntry>>>,
}

impl Storage for FakeStorage {
    fn list_projects(&self) -> Result<Vec<Project>, PortError> {
        Ok(self.projects.lock().unwrap().clone())
    }
    fn add_project(&self, name: &str, path: &Path, _now: i64) -> Result<Project, PortError> {
        let mut projects = self.projects.lock().unwrap();
        let id = ProjectId(i64::try_from(projects.len()).unwrap() + 1);
        let project = Project {
            id,
            name: name.into(),
            path: path.into(),
            collapsed: false,
        };
        projects.push(project.clone());
        Ok(project)
    }
    fn set_project_collapsed(&self, id: ProjectId, collapsed: bool) -> Result<(), PortError> {
        for p in self
            .projects
            .lock()
            .unwrap()
            .iter_mut()
            .filter(|p| p.id == id)
        {
            p.collapsed = collapsed;
        }
        Ok(())
    }
    fn remove_project(&self, id: ProjectId) -> Result<(), PortError> {
        self.projects.lock().unwrap().retain(|p| p.id != id);
        self.conversations
            .lock()
            .unwrap()
            .retain(|c| c.project != id);
        Ok(())
    }
    fn list_conversations(&self) -> Result<Vec<Conversation>, PortError> {
        Ok(self.conversations.lock().unwrap().clone())
    }
    fn list_sessions(&self) -> Result<Vec<AgentSession>, PortError> {
        Ok(self.sessions.lock().unwrap().clone())
    }
    fn create_conversation(
        &self,
        project: ProjectId,
        title: &str,
        now: i64,
    ) -> Result<Conversation, PortError> {
        let mut list = self.conversations.lock().unwrap();
        let c = Conversation {
            id: ConversationId(i64::try_from(list.len()).unwrap() + 1),
            project,
            title: title.into(),
            pinned: false,
            archived: false,
            unread: false,
            updated_at: now,
            working_dir: None,
            issue: None,
        };
        list.push(c.clone());
        Ok(c)
    }
    fn update_conversation(&self, c: &Conversation) -> Result<(), PortError> {
        let mut list = self.conversations.lock().unwrap();
        if let Some(slot) = list.iter_mut().find(|x| x.id == c.id) {
            *slot = c.clone();
        }
        Ok(())
    }
    fn delete_conversation(&self, id: ConversationId) -> Result<(), PortError> {
        self.conversations.lock().unwrap().retain(|c| c.id != id);
        Ok(())
    }
    fn create_session(&self, new: &NewSession) -> Result<AgentSession, PortError> {
        let mut list = self.sessions.lock().unwrap();
        let s = AgentSession {
            id: AgentSessionId(i64::try_from(list.len()).unwrap() + 1),
            conversation: new.conversation,
            started_by: new.started_by,
            settings: new.settings.clone(),
            working_dir: new.working_dir.clone(),
            cli_session: None,
        };
        list.push(s.clone());
        Ok(s)
    }
    fn update_session(&self, s: &AgentSession) -> Result<(), PortError> {
        let mut list = self.sessions.lock().unwrap();
        if let Some(slot) = list.iter_mut().find(|x| x.id == s.id) {
            *slot = s.clone();
        }
        Ok(())
    }
    fn append_entry(&self, session: AgentSessionId, entry: &TimedEntry) -> Result<(), PortError> {
        self.entries
            .lock()
            .unwrap()
            .entry(session)
            .or_default()
            .push(entry.clone());
        Ok(())
    }
    fn replace_entries(
        &self,
        session: AgentSessionId,
        entries: &[TimedEntry],
    ) -> Result<(), PortError> {
        self.entries
            .lock()
            .unwrap()
            .insert(session, entries.to_vec());
        Ok(())
    }
    fn entries(&self, session: AgentSessionId) -> Result<Vec<TimedEntry>, PortError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .get(&session)
            .cloned()
            .unwrap_or_default())
    }
}

/// One started CLI: what it was started with, the commands it got, and a
/// sender to play its events.
struct Started {
    spec: SessionSpec,
    commands: UnboundedReceiver<BackendCommand>,
    events: UnboundedSender<BackendEvent>,
}

#[derive(Default)]
struct FakeBackends {
    started: Mutex<Vec<Started>>,
    /// What `title` answers; `None`: it fails.
    title: Mutex<Option<String>>,
}

impl Backends for FakeBackends {
    fn start(&self, spec: SessionSpec) -> Result<BackendChannel, PortError> {
        let (commands, command_rx) = unbounded_channel();
        let (event_tx, events) = unbounded_channel();
        self.started.lock().unwrap().push(Started {
            spec,
            commands: command_rx,
            events: event_tx,
        });
        Ok(BackendChannel { commands, events })
    }
    fn options(&self, _backend: BackendKind) -> BackendOptions {
        BackendOptions::default()
    }
    fn title(&self, _backend: BackendKind, _message: &str) -> Result<String, PortError> {
        self.title
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| PortError::new("writing a title", "no title in this test"))
    }
    fn connectors(&self) -> Vec<leon_domain::CliConnectors> {
        vec![leon_domain::CliConnectors {
            backend: BackendKind::Codex,
            connectors: vec![leon_domain::Connector {
                name: "node_repl".into(),
                kind: leon_domain::ConnectorKind::Server,
                state: leon_domain::ConnectorState::On,
                detail: "node".into(),
            }],
            problem: None,
        }]
    }
    fn doctor(&self) -> Vec<leon_domain::DoctorSection> {
        vec![leon_domain::DoctorSection {
            title: "Codex".into(),
            checks: vec![leon_domain::DoctorCheck::new(
                leon_domain::Health::Problem,
                "Logged in",
                "no",
            )],
        }]
    }
}

struct FakeCatalog;

impl Catalog for FakeCatalog {
    fn commands(&self, _project: &Path) -> Vec<crate::ports::SlashCommand> {
        Vec::new()
    }
    fn sources(&self) -> Vec<crate::ports::Source> {
        Vec::new()
    }
    fn doctor(&self) -> Vec<leon_domain::DoctorCheck> {
        Vec::new()
    }
    fn agents(&self, _project: &Path) -> Vec<AgentDefinition> {
        vec![
            AgentDefinition {
                name: "planner".into(),
                description: "Plans".into(),
                instructions: "Plan carefully.".into(),
            },
            AgentDefinition {
                name: "coder".into(),
                description: "Codes".into(),
                instructions: "Code carefully.".into(),
            },
        ]
    }
}

/// Ports with fake catalog, git and GitHub around the given storage and CLIs.
fn ports(storage: Arc<dyn Storage>, backends: Arc<dyn Backends>) -> Ports {
    Ports {
        storage,
        backends,
        catalog: Arc::new(FakeCatalog),
        git: Arc::new(repo::FakeGit),
        github: Arc::new(repo::FakeGitHub::default()),
        app_tools: Some(Arc::new(agents::FakeTools)),
        shared_tools: Arc::new(tools::FakeShared::default()),
        config: Arc::new(tools::FakeConfig::default()),
        clock: || 1_000,
        about: Arc::new(|| {
            vec![leon_domain::DoctorCheck::new(
                leon_domain::Health::Info,
                "Version",
                "Leon 0.1.0",
            )]
        }),
    }
}

struct Harness {
    core: CoreHandle,
    storage: Arc<FakeStorage>,
    backends: Arc<FakeBackends>,
}

fn settings(backend: BackendKind) -> SessionSettings {
    SessionSettings {
        backend,
        agent: Some("planner".into()),
        model: None,
        effort: None,
        permission: "auto".into(),
    }
}

fn harness() -> Harness {
    harness_with_config(Arc::new(tools::FakeConfig::default()))
}

fn harness_with_config(config: Arc<dyn crate::ports::ConfigFile>) -> Harness {
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let backends = Arc::new(FakeBackends::default());
    let core = start(
        &tokio::runtime::Handle::current(),
        Ports {
            config,
            ..ports(
                Arc::clone(&storage) as Arc<dyn Storage>,
                Arc::clone(&backends) as Arc<dyn Backends>,
            )
        },
        Settings::default(),
    );
    Harness {
        core,
        storage,
        backends,
    }
}

async fn until(core: &CoreHandle, what: &str, check: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
    let mut rx = core.subscribe();
    match tokio::time::timeout(Duration::from_secs(2), rx.wait_for(|s| check(s))).await {
        Ok(Ok(s)) => Arc::clone(&s),
        _ => panic!("timed out waiting for: {what}"),
    }
}

async fn started(h: &Harness, count: usize) {
    for _ in 0..200 {
        if h.backends.started.lock().unwrap().len() >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the CLI was not started");
}

fn emit(h: &Harness, index: usize, event: BackendEvent) {
    h.backends.started.lock().unwrap()[index]
        .events
        .send(event)
        .unwrap();
}

async fn next_command(h: &Harness, index: usize) -> BackendCommand {
    for _ in 0..200 {
        if let Ok(c) = h.backends.started.lock().unwrap()[index]
            .commands
            .try_recv()
        {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("no command reached the CLI");
}

fn entries(s: &Snapshot, session: AgentSessionId) -> Vec<Entry> {
    s.transcripts
        .get(&session)
        .map_or_default(|t| t.entries.iter().map(|e| e.entry.clone()).collect())
}

async fn new_conversation(h: &Harness, backend: BackendKind) -> AgentSessionId {
    until(&h.core, "loaded", |s| s.loaded).await;
    h.core.send(Intent::NewConversation {
        project: ProjectId(1),
        settings: settings(backend),
        text: "Fix the retry bug".into(),
        issue: Some(leon_domain::GhRef::issue("acme/api", 12)),
    });
    let s = until(&h.core, "conversation", |s| !s.conversations.is_empty()).await;
    // The issue it was started for is kept with it (GH-2).
    assert_eq!(
        s.conversations[0].conversation.issue,
        Some(leon_domain::GhRef::issue("acme/api", 12))
    );
    started(h, 1).await;
    s.conversations[0].sessions[0]
}
