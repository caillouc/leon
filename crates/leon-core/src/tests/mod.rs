//! Core behaviour with fake storage, CLIs, git and GitHub.

mod agents;
mod archive;
mod editor;
mod folders;
mod held;
mod questions;
mod repo;
mod rewind;
mod todos;
mod tools;
mod turns;
mod update;

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
    todos: Mutex<Vec<leon_domain::Todo>>,
    plan_usage: Mutex<Vec<leon_domain::PlanUsage>>,
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
    fn set_project_order(&self, order: &[ProjectId]) -> Result<(), PortError> {
        self.projects
            .lock()
            .unwrap()
            .sort_by_key(|p| order.iter().position(|id| *id == p.id));
        Ok(())
    }
    fn set_todo_order(&self, order: &[FlowId]) -> Result<(), PortError> {
        for f in self.flows.lock().unwrap().iter_mut() {
            if let Some(at) = order.iter().position(|id| *id == f.id) {
                f.todo_position = i64::try_from(at).unwrap();
            }
        }
        Ok(())
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
    fn todos(&self) -> Result<Vec<leon_domain::Todo>, PortError> {
        Ok(self.todos.lock().unwrap().clone())
    }
    fn plan_usage(&self) -> Result<Vec<leon_domain::PlanUsage>, PortError> {
        Ok(self.plan_usage.lock().unwrap().clone())
    }
    fn save_plan_usage(&self, usage: &leon_domain::PlanUsage) -> Result<(), PortError> {
        let mut all = self.plan_usage.lock().unwrap();
        all.retain(|u| u.backend != usage.backend);
        all.push(usage.clone());
        Ok(())
    }
    fn add_todo(
        &self,
        flow: FlowId,
        (when, what): (&str, &str),
        by: Option<ConversationId>,
        now: i64,
    ) -> Result<leon_domain::Todo, PortError> {
        let mut todos = self.todos.lock().unwrap();
        let todo = leon_domain::Todo {
            id: leon_domain::TodoId(i64::try_from(todos.len()).unwrap() + 1),
            flow,
            when: when.into(),
            what: what.into(),
            done: false,
            by,
            added_at: now,
        };
        todos.push(todo.clone());
        Ok(todo)
    }
    fn edit_todo(
        &self,
        id: leon_domain::TodoId,
        (when, what): (&str, &str),
    ) -> Result<(), PortError> {
        for todo in self.todos.lock().unwrap().iter_mut().filter(|t| t.id == id) {
            (todo.when, todo.what) = (when.into(), what.into());
        }
        Ok(())
    }
    fn remove_todo(&self, id: leon_domain::TodoId) -> Result<(), PortError> {
        self.todos.lock().unwrap().retain(|t| t.id != id);
        Ok(())
    }
    fn restore_todo(&self, todo: &leon_domain::Todo) -> Result<(), PortError> {
        crate::todos::put_back(&mut self.todos.lock().unwrap(), todo.clone());
        Ok(())
    }
    fn set_todo_done(&self, id: leon_domain::TodoId, done: bool) -> Result<(), PortError> {
        for todo in self.todos.lock().unwrap().iter_mut().filter(|t| t.id == id) {
            todo.done = done;
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
            archived: None,
            unread: false,
            updated_at: now,
            folders: Vec::new(),
            links: Vec::new(),
            todo_position: 0,
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
            folder: None,
            tab_closed: false,
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
    /// CLIs still running from before Leon started (I24), and whether each
    /// was in a turn.
    held: Mutex<Vec<(ConversationId, bool)>>,
    /// What `title` answers; `None`: it fails.
    title: Mutex<Option<String>>,
    /// Copilot CLI's connectors wait for this, to test a slow CLI.
    copilot_answers: std::sync::atomic::AtomicBool,
    /// The conversations whose leftovers were removed, and their sessions.
    discarded: Mutex<Vec<(ConversationId, Option<crate::ports::CliSession>)>>,
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
    fn discard(&self, id: ConversationId, session: Option<crate::ports::CliSession>) {
        self.discarded.lock().unwrap().push((id, session));
    }
    /// Codex tells its plan without a turn: no window, 900 credits.
    fn plan_usage(&self, backend: BackendKind) -> Option<crate::ports::PlanSaid> {
        (backend == BackendKind::Codex).then(|| (Vec::new(), Some("900".to_owned())))
    }
    /// A pasted image is saved under its conversation; a dropped file named
    /// `broken` cannot be read.
    fn save_image(
        &self,
        id: ConversationId,
        image: &crate::ports::Image,
    ) -> Result<PathBuf, PortError> {
        match image {
            crate::ports::Image::Pasted { ext, .. } => {
                Ok(PathBuf::from(format!("/data/images/{}/1.{ext}", id.0)))
            }
            crate::ports::Image::File(path) if path.ends_with("broken") => {
                Err(PortError::new("saving an image", "no such file"))
            }
            crate::ports::Image::File(path) => {
                Ok(PathBuf::from(format!("/data/images/{}", id.0)).join(path.file_name().unwrap()))
            }
        }
    }
    fn held(&self) -> Vec<ConversationId> {
        self.held
            .lock()
            .unwrap()
            .iter()
            .map(|(id, _)| *id)
            .collect()
    }
    fn attach(&self, spec: SessionSpec) -> Option<crate::ports::Attached> {
        let busy = self
            .held
            .lock()
            .unwrap()
            .iter()
            .find(|(id, _)| *id == spec.conversation)?
            .1;
        let channel = self.start(spec).ok()?;
        Some(crate::ports::Attached { channel, busy })
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
        git: Arc::new(repo::FakeGit::default()),
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
        updater: Arc::new(update::FakeUpdater::default()),
        editors: Arc::new(editor::FakeEditors::default()),
    }
}

struct Harness {
    core: CoreHandle,
    storage: Arc<FakeStorage>,
    backends: Arc<FakeBackends>,
    git: Arc<repo::FakeGit>,
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
    let git = Arc::new(repo::FakeGit::default());
    let core = start(
        &tokio::runtime::Handle::current(),
        Ports {
            config,
            git: Arc::clone(&git) as Arc<dyn crate::ports::Git>,
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
        git,
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
        images: Vec::new(),
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
