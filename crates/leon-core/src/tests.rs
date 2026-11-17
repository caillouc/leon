//! Core behaviour with fake storage and fake CLIs.

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
        };
        projects.push(project.clone());
        Ok(project)
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
}

struct FakeCatalog;

impl Catalog for FakeCatalog {
    fn agents(&self, _project: &Path) -> Vec<AgentDefinition> {
        vec![AgentDefinition {
            name: "planner".into(),
            description: "Plans".into(),
            instructions: "Plan carefully.".into(),
        }]
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
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let backends = Arc::new(FakeBackends::default());
    let ports = Ports {
        storage: Arc::clone(&storage) as Arc<dyn Storage>,
        backends: Arc::clone(&backends) as Arc<dyn Backends>,
        catalog: Arc::new(FakeCatalog),
        clock: || 1_000,
    };
    let core = start(
        &tokio::runtime::Handle::current(),
        ports,
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
    });
    let s = until(&h.core, "conversation", |s| !s.conversations.is_empty()).await;
    started(h, 1).await;
    s.conversations[0].sessions[0]
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_streams_then_lands_in_the_transcript() {
    let h = harness();
    let id = new_conversation(&h, BackendKind::Codex).await;
    let spec = h.backends.started.lock().unwrap()[0].spec.clone();
    assert_eq!(spec.working_dir, PathBuf::from("/code/api"));
    assert_eq!(
        spec.agent.map(|a| a.instructions),
        Some("Plan carefully.".into())
    );
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Send("Fix the retry bug".into())
    );

    emit(
        &h,
        0,
        BackendEvent::Ready {
            cli_session: "thread-1".into(),
        },
    );
    emit(&h, 0, BackendEvent::TextDelta("Look".into()));
    let s = until(&h.core, "partial text", |s| {
        s.transcripts[&id].partial.as_deref() == Some("Look")
    })
    .await;
    assert_eq!(s.conversations[0].status, Status::Running);
    assert_eq!(s.conversations[0].conversation.title, "Fix the retry bug");

    emit(
        &h,
        0,
        BackendEvent::Tool {
            id: "t1".into(),
            name: "Ran".into(),
            detail: "cargo test".into(),
        },
    );
    emit(
        &h,
        0,
        BackendEvent::ToolDone {
            id: "t1".into(),
            output: "ok".into(),
            failed: false,
        },
    );
    emit(&h, 0, BackendEvent::Text("Looks fixed.".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    let s = until(&h.core, "idle", |s| {
        s.conversations[0].status == Status::Idle
    })
    .await;
    assert_eq!(
        entries(&s, id),
        [
            Entry::User {
                text: "Fix the retry bug".into()
            },
            Entry::Tool {
                id: "t1".into(),
                name: "Ran".into(),
                detail: "cargo test".into(),
                output: Some("ok".into()),
                failed: false
            },
            Entry::Agent {
                text: "Looks fixed.".into()
            },
            Entry::TurnEnd { usage: None },
        ]
    );
    assert_eq!(
        h.storage.sessions.lock().unwrap()[0].cli_session.as_deref(),
        Some("thread-1")
    );
    assert_eq!(h.storage.entries.lock().unwrap()[&id].len(), 4);
    assert!(
        !s.conversations[0].conversation.unread,
        "the conversation is on screen"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_off_screen_marks_unread_and_notifies() {
    let h = harness();
    let id = new_conversation(&h, BackendKind::Codex).await;
    h.core.send(Intent::View(None));
    emit(&h, 0, BackendEvent::Text("Done.".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    let s = until(&h.core, "unread", |s| {
        s.conversations[0].conversation.unread
    })
    .await;
    assert_eq!(s.notifications.len(), 1);
    assert_eq!(s.notifications[0].title, "planner answered");
    h.core
        .send(Intent::View(Some(s.conversations[0].conversation.id)));
    until(&h.core, "read", |s| !s.conversations[0].conversation.unread).await;
    assert_eq!(entries(&s, id).len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_and_questions_reach_the_cli() {
    let h = harness();
    let id = new_conversation(&h, BackendKind::Codex).await;
    next_command(&h, 0).await;
    h.core.send(Intent::Stop(id));
    assert_eq!(next_command(&h, 0).await, BackendCommand::Interrupt);

    emit(
        &h,
        0,
        BackendEvent::Question {
            id: "q1".into(),
            title: "Run it?".into(),
            detail: "rm -rf build".into(),
            options: vec!["Allow once".into(), "Deny".into()],
        },
    );
    let s = until(&h.core, "needs input", |s| {
        s.conversations[0].status == Status::NeedsInput
    })
    .await;
    assert_eq!(
        s.sessions[&id].question.as_ref().map(|q| q.detail.as_str()),
        Some("rm -rf build")
    );
    h.core.send(Intent::Answer {
        session: id,
        question: "q1".into(),
        answer: "Deny".into(),
    });
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Answer {
            id: "q1".into(),
            answer: "Deny".into()
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn switching_backend_restarts_with_the_conversation_so_far() {
    let h = harness();
    let id = new_conversation(&h, BackendKind::Codex).await;
    emit(
        &h,
        0,
        BackendEvent::Ready {
            cli_session: "thread-1".into(),
        },
    );
    emit(&h, 0, BackendEvent::Text("Plan: raise the limit.".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    until(&h.core, "idle", |s| {
        s.conversations[0].status == Status::Idle
    })
    .await;

    h.core.send(Intent::ChangeSettings {
        session: id,
        settings: settings(BackendKind::ClaudeCode),
    });
    h.core.send(Intent::Send {
        session: id,
        text: "Go ahead".into(),
    });
    started(&h, 2).await;
    let spec = h.backends.started.lock().unwrap()[1].spec.clone();
    assert_eq!(spec.settings.backend, BackendKind::ClaudeCode);
    assert_eq!(spec.resume, None);
    let BackendCommand::Send(message) = next_command(&h, 1).await else {
        panic!("expected a message")
    };
    assert!(
        message.contains("Assistant: Plan: raise the limit."),
        "{message}"
    );
    assert!(message.ends_with("Go ahead"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cli_that_cannot_start_shows_an_error() {
    struct Broken;
    impl Backends for Broken {
        fn start(&self, _spec: SessionSpec) -> Result<BackendChannel, PortError> {
            Err(PortError::new("starting Codex", "not installed"))
        }
        fn options(&self, _b: BackendKind) -> BackendOptions {
            BackendOptions::default()
        }
    }
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let ports = Ports {
        storage,
        backends: Arc::new(Broken),
        catalog: Arc::new(FakeCatalog),
        clock: || 1,
    };
    let core = start(
        &tokio::runtime::Handle::current(),
        ports,
        Settings::default(),
    );
    until(&core, "loaded", |s| s.loaded).await;
    core.send(Intent::NewConversation {
        project: ProjectId(1),
        settings: settings(BackendKind::Codex),
        text: "hi".into(),
    });
    let s = until(&core, "error", |s| {
        s.conversations
            .first()
            .is_some_and(|c| c.status == Status::Error)
    })
    .await;
    let id = s.conversations[0].sessions[0];
    assert!(
        matches!(entries(&s, id).last(), Some(Entry::Error { text }) if text.contains("not installed"))
    );
}
