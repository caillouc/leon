//! Core behaviour with fake storage, CLIs, git and GitHub.

mod agents;
mod folders;
mod repo;
mod rewind;
mod tools;
mod turns;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use leon_domain::{
    BackendKind, Conversation, ConversationId, ConversationSettings, Entry, Flow, FlowId, Project,
    ProjectId, Settings, Status, TimedEntry,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::ports::{
    AgentDefinition, BackendChannel, BackendCommand, BackendEvent, BackendOptions, Backends,
    Catalog, NewConversation, PortError, SessionSpec, Storage,
};
use crate::{CoreHandle, Intent, Ports, Snapshot, start};

#[derive(Default)]
struct FakeStorage {
    projects: Mutex<Vec<Project>>,
    flows: Mutex<Vec<Flow>>,
    conversations: Mutex<Vec<Conversation>>,
    entries: Mutex<HashMap<ConversationId, Vec<TimedEntry>>>,
    folded: Mutex<std::collections::BTreeSet<String>>,
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
    fn folded_groups(&self) -> Result<Vec<String>, PortError> {
        Ok(self.folded.lock().unwrap().iter().cloned().collect())
    }
    fn set_folded_group(&self, key: &str, folded: bool) -> Result<(), PortError> {
        let mut set = self.folded.lock().unwrap();
        if folded {
            set.insert(key.to_owned());
        } else {
            set.remove(key);
        }
        Ok(())
    }
    fn remove_project(&self, id: ProjectId) -> Result<(), PortError> {
        self.projects.lock().unwrap().retain(|p| p.id != id);
        self.flows.lock().unwrap().retain(|c| c.project != id);
        Ok(())
    }
    fn list_flows(&self) -> Result<Vec<Flow>, PortError> {
        Ok(self.flows.lock().unwrap().clone())
    }
    fn list_conversations(&self) -> Result<Vec<Conversation>, PortError> {
        Ok(self.conversations.lock().unwrap().clone())
    }
    fn create_flow(&self, project: ProjectId, title: &str, now: i64) -> Result<Flow, PortError> {
        let mut list = self.flows.lock().unwrap();
        let c = Flow {
            id: FlowId(i64::try_from(list.len()).unwrap() + 1),
            project,
            title: title.into(),
            pinned: false,
            archived: false,
            unread: false,
            updated_at: now,
            folders: Vec::new(),
            links: Vec::new(),
        };
        list.push(c.clone());
        Ok(c)
    }
    fn update_flow(&self, c: &Flow) -> Result<(), PortError> {
        let mut list = self.flows.lock().unwrap();
        if let Some(slot) = list.iter_mut().find(|x| x.id == c.id) {
            *slot = c.clone();
        }
        Ok(())
    }
    fn delete_flow(&self, id: FlowId) -> Result<(), PortError> {
        self.flows.lock().unwrap().retain(|c| c.id != id);
        Ok(())
    }
    fn create_conversation(&self, new: &NewConversation) -> Result<Conversation, PortError> {
        let mut list = self.conversations.lock().unwrap();
        let s = Conversation {
            id: ConversationId(i64::try_from(list.len()).unwrap() + 1),
            flow: new.flow,
            started_by: new.started_by,
            settings: new.settings.clone(),
            working_dir: new.working_dir.clone(),
            cli_session: None,
            unread: false,
        };
        list.push(s.clone());
        Ok(s)
    }
    fn update_conversation(&self, s: &Conversation) -> Result<(), PortError> {
        let mut list = self.conversations.lock().unwrap();
        if let Some(slot) = list.iter_mut().find(|x| x.id == s.id) {
            *slot = s.clone();
        }
        Ok(())
    }
    fn append_entry(
        &self,
        conversation: ConversationId,
        entry: &TimedEntry,
    ) -> Result<(), PortError> {
        self.entries
            .lock()
            .unwrap()
            .entry(conversation)
            .or_default()
            .push(entry.clone());
        Ok(())
    }
    fn replace_entries(
        &self,
        conversation: ConversationId,
        entries: &[TimedEntry],
    ) -> Result<(), PortError> {
        self.entries
            .lock()
            .unwrap()
            .insert(conversation, entries.to_vec());
        Ok(())
    }
    fn entries(&self, conversation: ConversationId) -> Result<Vec<TimedEntry>, PortError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .get(&conversation)
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
    /// Copilot CLI's connectors wait for this, to test a slow CLI.
    copilot_answers: std::sync::atomic::AtomicBool,
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
    fn connectors(&self, backend: BackendKind) -> leon_domain::CliConnectors {
        if backend == BackendKind::CopilotCli {
            for _ in 0..400 {
                if self
                    .copilot_answers
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        leon_domain::CliConnectors {
            backend,
            connectors: vec![leon_domain::Connector {
                name: format!("{}_server", backend.id()),
                kind: leon_domain::ConnectorKind::Server,
                state: leon_domain::ConnectorState::On,
                detail: "node".into(),
            }],
            problem: None,
        }
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
    fn skills(&self, _project: &Path) -> Vec<crate::ports::SkillFile> {
        Vec::new()
    }
    fn folders(&self) -> Vec<crate::ports::AgentFolder> {
        Vec::new()
    }
    fn instructions(&self) -> Option<crate::ports::SharedInstructions> {
        Some(crate::ports::SharedInstructions {
            file: PathBuf::from("/home/u/.agents/AGENTS.md"),
            text: "Be brief.".into(),
        })
    }
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
                ..Default::default()
            },
            AgentDefinition {
                name: "coder".into(),
                description: "Codes".into(),
                instructions: "Code carefully.".into(),
                ..Default::default()
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

fn settings(backend: BackendKind) -> ConversationSettings {
    ConversationSettings {
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

fn entries(s: &Snapshot, conversation: ConversationId) -> Vec<Entry> {
    s.transcripts
        .get(&conversation)
        .map_or_default(|t| t.entries.iter().map(|e| e.entry.clone()).collect())
}

async fn new_flow(h: &Harness, backend: BackendKind) -> ConversationId {
    until(&h.core, "loaded", |s| s.loaded).await;
    h.core.send(Intent::NewFlow {
        project: ProjectId(1),
        settings: settings(backend),
        text: "Fix the retry bug".into(),
        issue: Some(leon_domain::GhRef::issue("acme/api", 12)),
    });
    let s = until(&h.core, "conversation", |s| !s.flows.is_empty()).await;
    // The issue it was started for is kept with it (GH-2).
    assert_eq!(
        s.flows[0].flow.links,
        [leon_domain::GhRef::issue("acme/api", 12)]
    );
    started(h, 1).await;
    s.flows[0].conversations[0]
}
