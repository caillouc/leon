//! The core actor: applies intents and CLI events to the state, persists
//! them, and publishes snapshots. The only writer of application state.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use leon_domain::{
    AgentSessionId, BackendKind, ConversationId, Entry, SessionSettings, Settings, Status,
    TimedEntry,
};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::watch;

use crate::intent::Intent;
use crate::ports::{
    AppToolRequest, AppToolSender, AppTools, BackendCommand, BackendEvent, Backends, Catalog, Git,
    GitHub, NewSession, SessionSpec, Storage,
};
use crate::repo::Internal;
use crate::snapshot::Snapshot;
use crate::state::{SessionState, State, title_from};

/// Everything the core talks to.
pub struct Ports {
    /// Leon's own data.
    pub storage: Arc<dyn Storage>,
    /// The coding CLIs.
    pub backends: Arc<dyn Backends>,
    /// The user's agent files.
    pub catalog: Arc<dyn Catalog>,
    /// Git state of working directories.
    pub git: Arc<dyn Git>,
    /// GitHub, with the user's login.
    pub github: Arc<dyn GitHub>,
    /// The app's MCP server given to every agent session (P10); `None`: no
    /// app tools.
    pub app_tools: Option<Arc<dyn AppTools>>,
    /// Current time, Unix milliseconds.
    pub clock: fn() -> i64,
    /// Leon's own facts for the doctor.
    pub about: crate::doctor::About,
}

/// The UI's handle on the core.
#[derive(Debug, Clone)]
pub struct CoreHandle {
    snapshots: watch::Receiver<Arc<Snapshot>>,
    intents: UnboundedSender<Intent>,
    tools: AppToolSender,
}

impl CoreHandle {
    /// The latest snapshot.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshots.borrow())
    }

    /// A receiver woken each time a snapshot is published. Runtime-agnostic
    /// (P8): the UI awaits it on its own executor.
    pub fn subscribe(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.snapshots.clone()
    }

    /// Asks the core to do something. Ignored once the core has stopped.
    pub fn send(&self, intent: Intent) {
        let _ = self.intents.send(intent);
    }

    /// Where the app's MCP server sends agents' tool calls (P10).
    pub fn app_tools(&self) -> AppToolSender {
        self.tools.clone()
    }
}

/// Starts the core on `runtime` (multi-threaded) and returns the UI's handle.
pub fn start(runtime: &Handle, ports: Ports, settings: Settings) -> CoreHandle {
    let state = State {
        settings,
        ..State::default()
    };
    let (publish, snapshots) = watch::channel(Arc::new(state.snapshot()));
    let (intents, intent_rx) = unbounded_channel();
    let (event_tx, event_rx) = unbounded_channel();
    let (internal_tx, internal_rx) = unbounded_channel();
    let (tools, tool_rx) = unbounded_channel();
    let ticker = internal_tx.clone();
    runtime.spawn(async move {
        let mut every = tokio::time::interval(std::time::Duration::from_secs(120));
        every.tick().await;
        loop {
            every.tick().await;
            if ticker.send(Internal::Tick).is_err() {
                return;
            }
        }
    });
    let core = Core {
        ports,
        state,
        publish,
        live: HashMap::new(),
        idle_since: HashMap::new(),
        viewing: None,
        event_tx,
        internal_tx,
        git_in_flight: HashSet::new(),
        git_dirty: HashSet::new(),
        next_flash: 0,
    };
    runtime.spawn(core.run(intent_rx, event_rx, internal_rx, tool_rx));
    CoreHandle {
        snapshots,
        intents,
        tools,
    }
}

/// The other end of a [`CoreHandle`] made by [`detached`].
#[derive(Debug)]
pub struct Detached {
    /// Publishes snapshots to the handle.
    pub publish: watch::Sender<Arc<Snapshot>>,
    /// Intents sent through the handle.
    pub intents: UnboundedReceiver<Intent>,
}

/// A handle with no core behind it: the caller publishes snapshots and reads
/// intents. For UI previews and tests.
pub fn detached(snapshot: Snapshot) -> (CoreHandle, Detached) {
    let (publish, snapshots) = watch::channel(Arc::new(snapshot));
    let (intents, intent_rx) = unbounded_channel();
    let (tools, _) = unbounded_channel();
    (
        CoreHandle {
            snapshots,
            intents,
            tools,
        },
        Detached {
            publish,
            intents: intent_rx,
        },
    )
}

type SessionEvent = (AgentSessionId, Option<BackendEvent>);

pub(crate) struct Core {
    pub(crate) ports: Ports,
    pub(crate) state: State,
    publish: watch::Sender<Arc<Snapshot>>,
    pub(crate) live: HashMap<AgentSessionId, UnboundedSender<BackendCommand>>,
    /// When each live CLI finished its last turn.
    pub(crate) idle_since: HashMap<AgentSessionId, i64>,
    viewing: Option<ConversationId>,
    event_tx: UnboundedSender<SessionEvent>,
    pub(crate) internal_tx: UnboundedSender<Internal>,
    pub(crate) git_in_flight: HashSet<ConversationId>,
    pub(crate) git_dirty: HashSet<ConversationId>,
    next_flash: u64,
}

/// Runs a blocking store call without stalling the async workers.
fn blocking<T>(f: impl FnOnce() -> T) -> T {
    tokio::task::block_in_place(f)
}

impl Core {
    async fn run(
        mut self,
        mut intents: UnboundedReceiver<Intent>,
        mut events: UnboundedReceiver<SessionEvent>,
        mut internal: UnboundedReceiver<Internal>,
        mut tools: UnboundedReceiver<AppToolRequest>,
    ) {
        self.load();
        self.publish();
        self.refresh_all_git();
        self.refresh_lists();
        self.run_doctor();
        loop {
            tokio::select! {
                intent = intents.recv() => match intent {
                    Some(intent) => self.apply(intent),
                    None => break,
                },
                Some((session, event)) = events.recv() => self.on_event(session, event),
                Some(message) = internal.recv() => self.on_internal(message),
                Some(request) = tools.recv() => self.on_tool(request),
            }
            self.publish();
        }
    }

    /// Shows a short message to the user.
    pub(crate) fn flash(&mut self, message: String) {
        self.next_flash += 1;
        self.state.flash = Some((self.next_flash, message));
    }

    fn publish(&self) {
        self.publish.send_replace(Arc::new(self.state.snapshot()));
    }

    fn now(&self) -> i64 {
        (self.ports.clock)()
    }

    pub(crate) fn now_ms(&self) -> i64 {
        self.now()
    }

    fn report<T>(result: Result<T, crate::ports::PortError>) -> Option<T> {
        result
            .map_err(|error| tracing::error!(%error, "store"))
            .ok()
    }

    fn load(&mut self) {
        let storage = Arc::clone(&self.ports.storage);
        let (projects, conversations, sessions) = blocking(|| {
            (
                storage.list_projects(),
                storage.list_conversations(),
                storage.list_sessions(),
            )
        });
        self.state.projects = Arc::new(Self::report(projects).unwrap_or_default());
        self.state.conversations = Self::report(conversations).unwrap_or_default();
        for session in Self::report(sessions).unwrap_or_default() {
            self.state
                .sessions
                .insert(session.id, SessionState::new(session));
        }
        let options = BackendKind::ALL
            .into_iter()
            .map(|b| (b, self.ports.backends.options(b)))
            .collect();
        self.state.options = Arc::new(options);
        self.refresh_agents();
        let fold = self
            .state
            .settings
            .diff_fold
            .iter()
            .filter_map(|p| leon_domain::FoldPattern::parse(p))
            .collect();
        self.state.fold = Arc::new(fold);
        self.state.loaded = true;
    }

    fn refresh_agents(&mut self) {
        let catalog = Arc::clone(&self.ports.catalog);
        let projects = Arc::clone(&self.state.projects);
        let (agents, commands) = blocking(|| {
            projects
                .iter()
                .map(|p| {
                    (
                        (p.id, catalog.agents(&p.path)),
                        (p.id, catalog.commands(&p.path)),
                    )
                })
                .unzip()
        });
        self.state.agents = Arc::new(agents);
        self.state.commands = Arc::new(commands);
        let catalog = Arc::clone(&self.ports.catalog);
        self.state.sources = Arc::new(blocking(|| catalog.sources()));
    }

    fn apply(&mut self, intent: Intent) {
        match intent {
            Intent::AddProject(path) => self.add_project(&path),
            Intent::View(id) => self.view(id),
            Intent::NewConversation {
                project,
                settings,
                text,
            } => self.new_conversation(project, settings, &text),
            Intent::Send { session, text } => self.send(session, &text),
            Intent::Rewind {
                session,
                entry,
                text,
            } => self.rewind(session, entry, &text),
            Intent::Stop(session) => {
                if let Some(commands) = self.live.get(&session) {
                    let _ = commands.send(BackendCommand::Interrupt);
                }
            }
            Intent::Answer {
                session,
                question,
                answer,
            } => self.answer(session, question, &answer),
            Intent::ChangeSettings { session, settings } => {
                self.change_settings(session, &settings);
            }
            Intent::CollapseProject(id, collapsed) => self.collapse_project(id, collapsed),
            Intent::Rename(id, title) => self.edit(id, |c| title.trim().clone_into(&mut c.title)),
            Intent::Pin(id, pinned) => self.edit(id, |c| c.pinned = pinned),
            Intent::Archive(id, archived) => self.edit(id, |c| c.archived = archived),
            Intent::MarkUnread(id) => self.edit(id, |c| c.unread = true),
            Intent::Delete(id) => self.delete(id),
            Intent::RefreshGit(id) => self.refresh_git(id),
            Intent::RefreshGitHub => self.refresh_lists(),
            Intent::RunDoctor => self.run_doctor(),
            Intent::RequestCopilotReview(id) => self.request_review(id),
            Intent::CommentOnPr(id, body) => self.comment(id, body),
        }
    }

    fn add_project(&mut self, path: &Path) {
        let name = path.file_name().map_or_else(
            || path.to_string_lossy().into_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
        let storage = Arc::clone(&self.ports.storage);
        let now = self.now();
        if Self::report(blocking(|| storage.add_project(&name, path, now))).is_some() {
            self.state.projects =
                Arc::new(Self::report(blocking(|| storage.list_projects())).unwrap_or_default());
            self.refresh_agents();
        }
    }

    fn collapse_project(&mut self, id: leon_domain::ProjectId, collapsed: bool) {
        let storage = Arc::clone(&self.ports.storage);
        if Self::report(blocking(|| storage.set_project_collapsed(id, collapsed))).is_some() {
            for p in Arc::make_mut(&mut self.state.projects)
                .iter_mut()
                .filter(|p| p.id == id)
            {
                p.collapsed = collapsed;
            }
        }
    }

    fn view(&mut self, id: Option<ConversationId>) {
        self.viewing = id;
        let Some(id) = id else { return };
        if self
            .state
            .conversations
            .iter()
            .any(|c| c.id == id && c.unread)
        {
            self.edit(id, |c| c.unread = false);
        }
        self.refresh_git(id);
        self.refresh_pr(id);
        for session in self.state.sessions_of(id) {
            self.load_transcript(session);
        }
    }

    fn load_transcript(&mut self, id: AgentSessionId) {
        if self
            .state
            .sessions
            .get(&id)
            .is_none_or(|s| s.transcript.is_some())
        {
            return;
        }
        let storage = Arc::clone(&self.ports.storage);
        let entries = Self::report(blocking(|| storage.entries(id))).unwrap_or_default();
        if let Some(s) = self.state.sessions.get_mut(&id) {
            s.transcript = Some(crate::snapshot::TranscriptView {
                entries: Arc::new(entries),
                partial: None,
            });
        }
    }

    pub(crate) fn edit(
        &mut self,
        id: ConversationId,
        change: impl FnOnce(&mut leon_domain::Conversation),
    ) {
        let Some(c) = self.state.conversation_mut(id) else {
            return;
        };
        change(c);
        let saved = c.clone();
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.update_conversation(&saved)));
    }

    fn touch(&mut self, id: ConversationId, mark_unread: bool) {
        let now = self.now();
        let unread = mark_unread && self.viewing != Some(id);
        self.edit(id, |c| {
            c.updated_at = now;
            c.unread |= unread;
        });
    }

    fn new_conversation(
        &mut self,
        project: leon_domain::ProjectId,
        settings: SessionSettings,
        text: &str,
    ) {
        let Some(dir) = self.state.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let settings = self.explicit(settings);
        let storage = Arc::clone(&self.ports.storage);
        let now = self.now();
        let title = title_from(text);
        let backend = settings.backend;
        let Some(conversation) = Self::report(blocking(|| {
            storage.create_conversation(project, &title, now)
        })) else {
            return;
        };
        let new = NewSession {
            conversation: conversation.id,
            started_by: None,
            settings,
            working_dir: dir,
        };
        let Some(session) = Self::report(blocking(|| storage.create_session(&new))) else {
            return;
        };
        let id = session.id;
        let mut state = SessionState::new(session);
        state.transcript = Some(crate::snapshot::TranscriptView::default());
        self.state.sessions.insert(id, state);
        self.state.conversations.push(conversation.clone());
        self.viewing = Some(conversation.id);
        self.send(id, text);
        self.write_title(conversation.id, &conversation.title, backend, text);
    }

    /// Asks the CLI for a title in the background (NAV-9).
    fn write_title(&self, id: ConversationId, old: &str, backend: BackendKind, text: &str) {
        let backends = Arc::clone(&self.ports.backends);
        let tx = self.internal_tx.clone();
        let (old, text) = (old.to_owned(), text.to_owned());
        tokio::task::spawn_blocking(move || match backends.title(backend, &text) {
            Ok(title) => {
                let _ = tx.send(Internal::Title(id, old, title));
            }
            Err(error) => tracing::debug!(%error, "no title from the CLI"),
        });
    }

    fn record(&mut self, id: AgentSessionId, entry: Entry) -> Option<usize> {
        let at = self.now();
        let timed = TimedEntry { at, entry };
        if !matches!(timed.entry, Entry::Tool { output: None, .. }) {
            let storage = Arc::clone(&self.ports.storage);
            Self::report(blocking(|| storage.append_entry(id, &timed)));
        }
        Some(self.state.sessions.get_mut(&id)?.push(timed))
    }

    /// Sends the user's message (CONV-1).
    fn send(&mut self, id: AgentSessionId, text: &str) {
        self.deliver(
            id,
            Entry::User {
                text: text.to_owned(),
            },
            text,
        );
    }

    /// Records `entry` in a session's transcript and sends `message` to its
    /// CLI, starting it if needed. `false` when nothing was sent: the session
    /// is busy, the message empty, or the CLI could not start.
    pub(crate) fn deliver(&mut self, id: AgentSessionId, entry: Entry, message: &str) -> bool {
        let Some(s) = self.state.sessions.get(&id) else {
            return false;
        };
        if s.status == Status::Running || message.trim().is_empty() {
            return false;
        }
        let conversation = s.session.conversation;
        let message = if s.carry_context {
            with_context(s, message)
        } else {
            message.to_owned()
        };
        self.load_transcript(id);
        self.record(id, entry);
        if !self.ensure_live(id) {
            return false;
        }
        if let Some(s) = self.state.sessions.get_mut(&id) {
            s.carry_context = false;
            s.status = Status::Running;
            s.question = None;
            s.suggestions.clear();
        }
        if let Some(commands) = self.live.get(&id) {
            let _ = commands.send(BackendCommand::Send(message));
        }
        self.idle_since.remove(&id);
        self.touch(conversation, false);
        true
    }

    /// Starts the CLI for a session if it is not running. Returns `false`
    /// (and records the error) when it cannot start.
    fn ensure_live(&mut self, id: AgentSessionId) -> bool {
        if self.live.get(&id).is_some_and(|c| !c.is_closed()) {
            return true;
        }
        let Some(s) = self.state.sessions.get(&id) else {
            return false;
        };
        let session = &s.session;
        let agent = session.settings.agent.as_ref().and_then(|name| {
            let project = self
                .state
                .conversations
                .iter()
                .find(|c| c.id == session.conversation)?
                .project;
            self.state
                .agents
                .get(&project)?
                .iter()
                .find(|a| &a.name == name)
                .cloned()
        });
        let spec = SessionSpec {
            settings: session.settings.clone(),
            agent,
            working_dir: session.working_dir.clone(),
            resume: session.cli_session.clone(),
            mcp_servers: self
                .ports
                .app_tools
                .as_ref()
                .map_or_default(|tools| vec![tools.server(id)]),
            sources: self.state.sources.to_vec(),
        };
        match self.ports.backends.start(spec) {
            Ok(channel) => {
                self.live.insert(id, channel.commands);
                let tx = self.event_tx.clone();
                let mut events = channel.events;
                tokio::spawn(async move {
                    while let Some(event) = events.recv().await {
                        if tx.send((id, Some(event))).is_err() {
                            return;
                        }
                    }
                    let _ = tx.send((id, None));
                });
                true
            }
            Err(error) => {
                self.record(
                    id,
                    Entry::Error {
                        text: error.to_string(),
                    },
                );
                if let Some(s) = self.state.sessions.get_mut(&id) {
                    s.status = Status::Error;
                }
                false
            }
        }
    }

    /// Goes back to before entry `entry`, a message of the user, and sends
    /// `text` (CONV-7, CONV-8). Works the same on every CLI: the next message
    /// starts a new CLI session that carries the conversation so far.
    fn rewind(&mut self, id: AgentSessionId, entry: usize, text: &str) {
        self.load_transcript(id);
        let Some(s) = self.state.sessions.get_mut(&id) else {
            return;
        };
        let Some(transcript) = s.transcript.as_mut() else {
            return;
        };
        let is_user = matches!(
            transcript.entries.get(entry).map(|e| &e.entry),
            Some(Entry::User { .. })
        );
        if s.status == Status::Running || !is_user || text.trim().is_empty() {
            return;
        }
        let entries = Arc::make_mut(&mut transcript.entries);
        let dropped = entries.len() - entry;
        entries.truncate(entry);
        let kept = entries.clone();
        s.session.cli_session = None;
        s.carry_context = true;
        s.suggestions.clear();
        self.live.remove(&id);
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.replace_entries(id, &kept)));
        self.save_session(id);
        self.record(
            id,
            Entry::Notice {
                text: format!(
                    "Went back: {dropped} later entries removed. Files the agent changed stay as they are."
                ),
            },
        );
        self.send(id, text);
    }

    fn answer(&mut self, id: AgentSessionId, question: String, answer: &str) {
        if let Some(commands) = self.live.get(&id) {
            let _ = commands.send(BackendCommand::Answer {
                id: question,
                answer: answer.to_owned(),
            });
        }
        self.record(
            id,
            Entry::Notice {
                text: format!("You answered: {answer}"),
            },
        );
        if let Some(s) = self.state.sessions.get_mut(&id) {
            s.question = None;
            s.status = Status::Running;
        }
    }

    /// Settings with the backend's default model and effort filled in.
    fn explicit(&self, mut settings: SessionSettings) -> SessionSettings {
        if let Some(options) = self.state.options.get(&settings.backend) {
            options.complete(&mut settings);
        }
        settings
    }

    fn change_settings(&mut self, id: AgentSessionId, settings: &SessionSettings) {
        let settings = &self.explicit(settings.clone());
        let Some(s) = self.state.sessions.get_mut(&id) else {
            return;
        };
        if &s.session.settings == settings {
            return;
        }
        let old = std::mem::replace(&mut s.session.settings, settings.clone());
        if old.backend != settings.backend {
            s.session.cli_session = None;
            s.carry_context = true;
        }
        let saved = s.session.clone();
        // The CLI reads these choices at start: the next message restarts it.
        self.live.remove(&id);
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.update_session(&saved)));
        self.record(
            id,
            Entry::Notice {
                text: describe_change(&old, settings),
            },
        );
    }

    fn delete(&mut self, id: ConversationId) {
        for session in self.state.sessions_of(id) {
            self.live.remove(&session);
            self.state.sessions.remove(&session);
        }
        self.state.conversations.retain(|c| c.id != id);
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.delete_conversation(id)));
    }

    fn on_event(&mut self, id: AgentSessionId, event: Option<BackendEvent>) {
        let Some(event) = event else {
            self.live.remove(&id);
            if self
                .state
                .sessions
                .get(&id)
                .is_some_and(|s| s.status == Status::Running)
            {
                self.finish_turn(
                    id,
                    None,
                    Some("The CLI stopped before finishing.".to_owned()),
                );
            }
            return;
        };
        crate::events::apply(self, id, event);
    }

    pub(crate) fn finish_turn(
        &mut self,
        id: AgentSessionId,
        usage: Option<leon_domain::Usage>,
        error: Option<String>,
    ) {
        let Some(s) = self.state.sessions.get_mut(&id) else {
            return;
        };
        let partial = s.transcript.as_mut().and_then(|t| t.partial.take());
        let pending: Vec<usize> = s.pending_tools.drain().map(|(_, i)| i).collect();
        let conversation = s.session.conversation;
        let failed = error.is_some() || s.status == Status::Error;
        s.status = if failed { Status::Error } else { Status::Idle };
        let storage = Arc::clone(&self.ports.storage);
        let entries: Vec<TimedEntry> = s.transcript.as_ref().map_or_default(|t| {
            pending
                .iter()
                .filter_map(|&i| t.entries.get(i).cloned())
                .collect()
        });
        for entry in &entries {
            Self::report(blocking(|| storage.append_entry(id, entry)));
        }
        if let Some(text) = partial.filter(|t| !t.trim().is_empty()) {
            self.record(id, Entry::Agent { text });
        }
        if let Some(text) = error {
            self.record(id, Entry::Error { text });
        }
        self.record(id, Entry::TurnEnd { usage });
        let now = self.now();
        self.idle_since.insert(id, now);
        self.refresh_git(conversation);
        if self.after_turn(id, failed) {
            self.touch(conversation, false);
        } else {
            self.answered(id, conversation, failed);
        }
    }

    /// The agent finished or needs the user: mark unread and notify when the
    /// conversation is not on screen (NAV-4, NOTIF-1).
    pub(crate) fn answered(
        &mut self,
        id: AgentSessionId,
        conversation: ConversationId,
        failed: bool,
    ) {
        self.touch(conversation, true);
        if self.viewing == Some(conversation) {
            return;
        }
        let agent = self
            .state
            .sessions
            .get(&id)
            .and_then(|s| s.session.settings.agent.clone())
            .unwrap_or_else(|| "The agent".to_owned());
        let title = self
            .state
            .conversations
            .iter()
            .find(|c| c.id == conversation)
            .map_or_default(|c| c.title.clone());
        let headline = if failed {
            format!("{agent} stopped with an error")
        } else {
            format!("{agent} answered")
        };
        self.state.notify(conversation, headline, title);
    }

    pub(crate) fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }

    pub(crate) fn record_entry(&mut self, id: AgentSessionId, entry: Entry) -> Option<usize> {
        self.record(id, entry)
    }

    pub(crate) fn save_session(&mut self, id: AgentSessionId) {
        let Some(saved) = self.state.sessions.get(&id).map(|s| s.session.clone()) else {
            return;
        };
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.update_session(&saved)));
    }

    pub(crate) fn save_entry(&mut self, id: AgentSessionId, entry: &TimedEntry) {
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.append_entry(id, entry)));
    }
}

/// The message for a new CLI session that must know the conversation so far.
fn with_context(s: &SessionState, text: &str) -> String {
    let mut lines = Vec::new();
    for e in s
        .transcript
        .as_ref()
        .map_or_default(|t| t.entries.as_slice())
    {
        match &e.entry {
            Entry::User { text } => lines.push(format!("User: {text}")),
            Entry::FromAgent { from, text, .. } => {
                lines.push(format!("Agent session {}: {text}", from.0));
            }
            Entry::Agent { text } => lines.push(format!("Assistant: {text}")),
            _ => {}
        }
    }
    let mut context = lines.join("\n\n");
    if context.len() > 24_000 {
        let start = context.len() - 24_000;
        let start = (start..context.len())
            .find(|&i| context.is_char_boundary(i))
            .unwrap_or(0);
        context = context[start..].to_owned();
    }
    format!(
        "Context: this conversation started with another assistant. Here it is so far:\n\n{context}\n\n---\n\n{text}"
    )
}

fn describe_change(old: &SessionSettings, new: &SessionSettings) -> String {
    if old.backend != new.backend {
        return format!(
            "Switched from {} to {}. Your next message starts a new {} session with this conversation's context.",
            old.backend.display_name(),
            new.backend.display_name(),
            new.backend.display_name()
        );
    }
    let mut parts = Vec::new();
    if old.model != new.model {
        parts.push(format!(
            "model {}",
            new.model.as_deref().unwrap_or("default")
        ));
    }
    if old.effort != new.effort {
        parts.push(format!(
            "effort {}",
            new.effort.as_deref().unwrap_or("default")
        ));
    }
    if old.permission != new.permission {
        parts.push(format!("permission {}", new.permission));
    }
    format!("Changed {} from your next message.", parts.join(", "))
}
