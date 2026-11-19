//! The core actor: applies intents and CLI events to the state, persists
//! them, and publishes snapshots. The only writer of application state.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use leon_domain::{
    BackendKind, ConversationId, ConversationSettings, Entry, FlowId, Settings, Status, TimedEntry,
};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::watch;

use crate::intent::Intent;
use crate::ports::{
    AppToolRequest, AppToolSender, AppTools, BackendCommand, BackendEvent, Backends, Catalog, Git,
    GitHub, NewConversation, SessionSpec, Storage,
};
use crate::repo::Internal;
use crate::snapshot::Snapshot;
use crate::state::{ConversationState, State, title_from};

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
    /// The app's MCP server given to every conversation (P10); `None`: no
    /// app tools.
    pub app_tools: Option<Arc<dyn AppTools>>,
    /// Leon's shared MCP list (I14).
    pub shared_tools: Arc<dyn crate::ports::SharedTools>,
    /// The user's config file, written by Settings.
    pub config: Arc<dyn crate::ports::ConfigFile>,
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
        settings_path: ports.config.path().map(Path::to_path_buf),
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
        reading: None,
        window_active: true,
        event_tx,
        internal_tx,
        git_in_flight: HashSet::new(),
        git_dirty: HashSet::new(),
        fetched: HashMap::new(),
        working_trees: HashMap::new(),
        lookups: 0,
        latest_lookup: HashMap::new(),
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

type ConversationEvent = (ConversationId, Option<BackendEvent>);

pub(crate) struct Core {
    pub(crate) ports: Ports,
    pub(crate) state: State,
    publish: watch::Sender<Arc<Snapshot>>,
    pub(crate) live: HashMap<ConversationId, UnboundedSender<BackendCommand>>,
    /// When each live CLI finished its last turn.
    pub(crate) idle_since: HashMap<ConversationId, i64>,
    viewing: Option<FlowId>,
    /// The conversation whose messages are on screen.
    reading: Option<ConversationId>,
    /// Leon's window is the active one.
    window_active: bool,
    event_tx: UnboundedSender<ConversationEvent>,
    pub(crate) internal_tx: UnboundedSender<Internal>,
    pub(crate) git_in_flight: HashSet<std::path::PathBuf>,
    pub(crate) git_dirty: HashSet<std::path::PathBuf>,
    /// When each folder's base branch was last fetched, in ms (I22).
    pub(crate) fetched: HashMap<(std::path::PathBuf, String), i64>,
    /// The working tree holding each folder agents changed files in (CTX-2).
    pub(crate) working_trees: HashMap<std::path::PathBuf, std::path::PathBuf>,
    /// Folder lookups started, and each conversation's latest: a lookup
    /// that ends after a later one does not move its conversation back.
    pub(crate) lookups: u64,
    pub(crate) latest_lookup: HashMap<ConversationId, u64>,
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
        mut events: UnboundedReceiver<ConversationEvent>,
        mut internal: UnboundedReceiver<Internal>,
        mut tools: UnboundedReceiver<AppToolRequest>,
    ) {
        self.load();
        self.publish();
        self.refresh_all_git();
        self.refresh_links();
        self.refresh_lists();
        self.run_doctor();
        loop {
            tokio::select! {
                intent = intents.recv() => match intent {
                    Some(intent) => self.apply(intent),
                    None => break,
                },
                Some((conversation, event)) = events.recv() => self.on_event(conversation, event),
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
        let (projects, flows, conversations) = blocking(|| {
            (
                storage.list_projects(),
                storage.list_flows(),
                storage.list_conversations(),
            )
        });
        self.state.projects = Arc::new(Self::report(projects).unwrap_or_default());
        let folded = blocking(|| storage.folded_groups());
        self.state.folded_groups = Arc::new(
            Self::report(folded)
                .unwrap_or_default()
                .into_iter()
                .collect(),
        );
        self.state.flows = Self::report(flows).unwrap_or_default();
        for conversation in Self::report(conversations).unwrap_or_default() {
            self.state
                .conversations
                .insert(conversation.id, ConversationState::new(conversation));
        }
        let options = BackendKind::ALL
            .into_iter()
            .map(|b| (b, self.ports.backends.options(b)))
            .collect();
        self.state.options = Arc::new(options);
        self.refresh_agents();
        self.apply_fold();
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
        let catalog = Arc::clone(&self.ports.catalog);
        self.state.folders = Arc::new(blocking(|| catalog.folders()));
        self.load_shared();
        let (git, projects) = (
            Arc::clone(&self.ports.git),
            Arc::clone(&self.state.projects),
        );
        let repos = blocking(|| {
            projects
                .iter()
                .filter_map(|p| Some((p.id, git.github_repo(&p.path)?)))
                .collect()
        });
        self.state.project_repos = Arc::new(repos);
    }

    fn apply(&mut self, intent: Intent) {
        match intent {
            Intent::AddProject(path) => self.add_project(&path),
            Intent::View(id) => self.view(id),
            Intent::NewFlow {
                project,
                settings,
                text,
                issue,
            } => self.new_flow(project, settings, &text, issue),
            Intent::NewConversation {
                flow,
                settings,
                working_dir,
                text,
            } => self.new_conversation(flow, settings, working_dir, &text),
            Intent::Send { conversation, text } => self.send(conversation, &text),
            Intent::Rewind {
                conversation,
                entry,
                text,
            } => self.rewind(conversation, entry, &text),
            Intent::Stop(conversation) => {
                if let Some(commands) = self.live.get(&conversation) {
                    let _ = commands.send(BackendCommand::Interrupt);
                }
            }
            Intent::Answer {
                conversation,
                question,
                answer,
            } => self.answer(conversation, question, &answer),
            Intent::ChangeSettings {
                conversation,
                settings,
            } => {
                self.change_settings(conversation, &settings);
            }
            Intent::CollapseProject(id, collapsed) => self.collapse_project(id, collapsed),
            Intent::RemoveProject(id) => self.remove_project(id),
            Intent::FoldGroup(key, folded) => self.fold_group(key, folded),
            Intent::Rename(id, title) => self.edit(id, |c| title.trim().clone_into(&mut c.title)),
            Intent::Pin(id, pinned) => self.edit(id, |c| c.pinned = pinned),
            Intent::Archive(id, archived) => self.edit(id, |c| c.archived = archived),
            Intent::MarkUnread(id) => self.edit(id, |c| c.unread = true),
            Intent::MarkConversationUnread(id) => self.mark_unread(id),
            Intent::WindowActive(active) => {
                self.window_active = active;
                // Back in Leon on a conversation's tab: its answer is seen
                // (NAV-4).
                if let Some(id) = self.reading.filter(|_| active) {
                    self.mark_read(id);
                }
            }
            Intent::Read(id) => self.read(id),
            Intent::Delete(id) => self.delete(id),
            Intent::RefreshGit(id) => {
                self.refresh_git(id);
                self.fetch_bases(id);
            }
            Intent::RefreshGitHub => self.refresh_lists(),
            Intent::RunDoctor => self.run_doctor(),
            Intent::RefreshTools => self.refresh_tools(),
            Intent::AddTool(server) => self.add_tool(server),
            Intent::RemoveTool(name) => self.remove_tool(&name),
            Intent::SaveSettings(settings) => self.save_settings(settings),
            Intent::RequestCopilotReview(item) => self.request_review(item),
            Intent::Comment(item, body) => self.comment(item, body),
            Intent::SetDraft(item, draft) => self.set_draft(item, draft),
            Intent::ReplyToThread {
                pr,
                thread,
                body,
                resolve,
            } => self.reply_to_thread(pr, thread, body, resolve),
            Intent::ResolveThread { pr, thread } => self.resolve_thread(pr, thread),
            Intent::ShowDetail(item) => self.show_detail(item),
            Intent::ShowPrDiff(item) => self.show_pr_diff(item),
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

    /// The user reads conversation `id`'s messages.
    fn read(&mut self, id: Option<ConversationId>) {
        self.reading = id;
        if let Some(id) = id {
            self.mark_read(id);
        }
    }

    /// The user marks conversation `id` unread: it is to review, and so is
    /// its flow, until they open its tab (NAV-4).
    fn mark_unread(&mut self, id: ConversationId) {
        let Some(s) = self.state.conversations.get_mut(&id) else {
            return;
        };
        let flow = s.conversation.flow;
        s.conversation.unread = true;
        self.save_conversation(id);
        self.edit(flow, |c| c.unread = true);
    }

    /// The user saw conversation `id`'s answer: it is read, and so is its
    /// flow once no other conversation's answer waits (NAV-4, FLOW-4).
    fn mark_read(&mut self, id: ConversationId) {
        let Some(s) = self.state.conversations.get_mut(&id) else {
            return;
        };
        let flow = s.conversation.flow;
        if std::mem::take(&mut s.conversation.unread) {
            self.save_conversation(id);
        }
        let waiting = self
            .state
            .conversations
            .values()
            .any(|s| s.conversation.flow == flow && s.conversation.unread);
        if self
            .state
            .flows
            .iter()
            .any(|c| c.id == flow && c.unread != waiting)
        {
            self.edit(flow, |c| c.unread = waiting);
        }
    }

    fn view(&mut self, id: Option<FlowId>) {
        self.viewing = id;
        let Some(id) = id else { return };
        self.refresh_git(id);
        self.fetch_bases(id);
        self.refresh_pr(id);
        for conversation in self.state.conversations_of(id) {
            self.load_transcript(conversation);
        }
    }

    fn load_transcript(&mut self, id: ConversationId) {
        if self
            .state
            .conversations
            .get(&id)
            .is_none_or(|s| s.transcript.is_some())
        {
            return;
        }
        let storage = Arc::clone(&self.ports.storage);
        let entries = Self::report(blocking(|| storage.entries(id))).unwrap_or_default();
        if let Some(s) = self.state.conversations.get_mut(&id) {
            s.transcript = Some(crate::snapshot::TranscriptView {
                entries: Arc::new(entries),
                partial: None,
            });
        }
    }

    pub(crate) fn edit(&mut self, id: FlowId, change: impl FnOnce(&mut leon_domain::Flow)) {
        let Some(c) = self.state.flow_mut(id) else {
            return;
        };
        change(c);
        let saved = c.clone();
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.update_flow(&saved)));
    }

    /// Whether the user sees conversation `id`'s messages: its tab, in the
    /// active window.
    fn on_screen(&self, id: ConversationId) -> bool {
        self.window_active && self.reading == Some(id)
    }

    fn touch(&mut self, id: FlowId, unread: bool) {
        let now = self.now();
        self.edit(id, |c| {
            c.updated_at = now;
            c.unread |= unread;
        });
    }

    fn new_flow(
        &mut self,
        project: leon_domain::ProjectId,
        settings: ConversationSettings,
        text: &str,
        issue: Option<leon_domain::GhRef>,
    ) {
        let Some(dir) = self.state.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let settings = self.explicit(settings);
        let storage = Arc::clone(&self.ports.storage);
        let now = self.now();
        let title = title_from(text);
        let backend = settings.backend;
        let Some(mut flow) = Self::report(blocking(|| storage.create_flow(project, &title, now)))
        else {
            return;
        };
        if let Some(issue) = issue {
            flow.links.push(issue);
            Self::report(blocking(|| storage.update_flow(&flow)));
        }
        let new = NewConversation {
            flow: flow.id,
            started_by: None,
            settings,
            working_dir: dir,
        };
        let Some(id) = Self::report(self.add_conversation(&new)) else {
            return;
        };
        self.state.flows.push(flow.clone());
        self.viewing = Some(flow.id);
        self.send(id, text);
        self.write_title(flow.id, &flow.title, backend, text);
    }

    /// The user starts another conversation in flow `conversation`
    /// (FLOW-7).
    fn new_conversation(
        &mut self,
        flow: FlowId,
        settings: ConversationSettings,
        working_dir: std::path::PathBuf,
        text: &str,
    ) {
        if !self.state.flows.iter().any(|c| c.id == flow) {
            return;
        }
        let new = NewConversation {
            flow,
            started_by: None,
            settings: self.explicit(settings),
            working_dir,
        };
        if let Some(id) = Self::report(self.add_conversation(&new)) {
            self.send(id, text);
        }
    }

    /// Saves a new agent session and keeps it, its transcript empty.
    pub(crate) fn add_conversation(
        &mut self,
        new: &NewConversation,
    ) -> Result<ConversationId, crate::ports::PortError> {
        let storage = Arc::clone(&self.ports.storage);
        let conversation = blocking(|| storage.create_conversation(new))?;
        let id = conversation.id;
        let mut state = ConversationState::new(conversation);
        state.transcript = Some(crate::snapshot::TranscriptView::default());
        self.state.conversations.insert(id, state);
        Ok(id)
    }

    /// Asks the CLI for a title in the background (NAV-9).
    fn write_title(&self, id: FlowId, old: &str, backend: BackendKind, text: &str) {
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

    fn record(&mut self, id: ConversationId, entry: Entry) -> Option<usize> {
        let at = self.now();
        let timed = TimedEntry { at, entry };
        if !matches!(timed.entry, Entry::Tool { output: None, .. }) {
            let storage = Arc::clone(&self.ports.storage);
            Self::report(blocking(|| storage.append_entry(id, &timed)));
        }
        Some(self.state.conversations.get_mut(&id)?.push(timed))
    }

    /// Sends the user's message (CONV-1).
    fn send(&mut self, id: ConversationId, text: &str) {
        // Replying means its answer was read.
        self.mark_read(id);
        self.deliver(
            id,
            Entry::User {
                text: text.to_owned(),
            },
            text,
        );
    }

    /// Records `entry` in a conversation's transcript and sends `message` to its
    /// CLI, starting it if needed. `false` when nothing was sent: the conversation
    /// is busy, the message empty, or the CLI could not start.
    pub(crate) fn deliver(&mut self, id: ConversationId, entry: Entry, message: &str) -> bool {
        let Some(s) = self.state.conversations.get(&id) else {
            return false;
        };
        if s.status == Status::Running || message.trim().is_empty() {
            return false;
        }
        let flow = s.conversation.flow;
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
        if let Some(s) = self.state.conversations.get_mut(&id) {
            s.carry_context = false;
            s.status = Status::Running;
            s.question = None;
            s.suggestions.clear();
        }
        if let Some(commands) = self.live.get(&id) {
            let _ = commands.send(BackendCommand::Send(message));
        }
        self.idle_since.remove(&id);
        self.touch(flow, false);
        true
    }

    /// Starts the CLI for a conversation if it is not running. Returns `false`
    /// (and records the error) when it cannot start.
    fn ensure_live(&mut self, id: ConversationId) -> bool {
        if self.live.get(&id).is_some_and(|c| !c.is_closed()) {
            return true;
        }
        let Some(s) = self.state.conversations.get(&id) else {
            return false;
        };
        let conversation = &s.conversation;
        let project = self
            .state
            .flows
            .iter()
            .find(|c| c.id == conversation.flow)
            .and_then(|c| self.state.project(c.project));
        let agent = conversation.settings.agent.as_ref().and_then(|name| {
            self.state
                .agents
                .get(&project?.id)?
                .iter()
                .find(|a| &a.name == name)
                .cloned()
        });
        // Read at each start: a skill file added since shows up (EXT-5).
        let catalog = Arc::clone(&self.ports.catalog);
        let project_dir = project.map(|p| p.path.clone());
        let (skills, instructions) = blocking(|| {
            (
                project_dir.map_or_default(|dir| catalog.skills(&dir)),
                catalog.instructions(),
            )
        });
        let spec = SessionSpec {
            settings: conversation.settings.clone(),
            agent,
            working_dir: conversation.working_dir.clone(),
            resume: conversation.cli_session.clone(),
            mcp_servers: self
                .conversation_servers(self.ports.app_tools.as_ref().map(|t| t.server(id))),
            sources: self.state.sources.to_vec(),
            skills,
            instructions,
            app_instructions: self.ports.app_tools.as_ref().map(|t| t.instructions()),
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
                if let Some(s) = self.state.conversations.get_mut(&id) {
                    s.status = Status::Error;
                }
                false
            }
        }
    }

    /// Goes back to before entry `entry`, a message of the user, and sends
    /// `text` (CONV-7, CONV-8). Works the same on every CLI: the next message
    /// starts a new CLI session that carries the flow so far.
    fn rewind(&mut self, id: ConversationId, entry: usize, text: &str) {
        self.load_transcript(id);
        let Some(s) = self.state.conversations.get_mut(&id) else {
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
        s.conversation.cli_session = None;
        s.carry_context = true;
        s.suggestions.clear();
        self.live.remove(&id);
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.replace_entries(id, &kept)));
        self.save_conversation(id);
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

    fn answer(&mut self, id: ConversationId, question: String, answer: &str) {
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
        if let Some(s) = self.state.conversations.get_mut(&id) {
            s.question = None;
            s.status = Status::Running;
        }
    }

    /// Settings with the backend's default model and effort filled in.
    fn explicit(&self, mut settings: ConversationSettings) -> ConversationSettings {
        if let Some(options) = self.state.options.get(&settings.backend) {
            options.complete(&mut settings);
        }
        settings
    }

    fn change_settings(&mut self, id: ConversationId, settings: &ConversationSettings) {
        let settings = &self.explicit(settings.clone());
        let Some(s) = self.state.conversations.get_mut(&id) else {
            return;
        };
        if &s.conversation.settings == settings {
            return;
        }
        let old = std::mem::replace(&mut s.conversation.settings, settings.clone());
        if old.backend != settings.backend {
            s.conversation.cli_session = None;
            s.carry_context = true;
        }
        let saved = s.conversation.clone();
        // The CLI reads these choices at start: the next message restarts it.
        self.live.remove(&id);
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.update_conversation(&saved)));
        self.record(
            id,
            Entry::Notice {
                text: describe_change(&old, settings),
            },
        );
    }

    fn delete(&mut self, id: FlowId) {
        self.forget(id);
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.delete_flow(id)));
    }

    /// Drops a flow and its conversations from memory, stopping their CLIs.
    fn forget(&mut self, id: FlowId) {
        for conversation in self.state.conversations_of(id) {
            self.live.remove(&conversation);
            self.state.conversations.remove(&conversation);
        }
        self.state.flows.retain(|c| c.id != id);
    }

    fn fold_group(&mut self, key: String, folded: bool) {
        let storage = Arc::clone(&self.ports.storage);
        if Self::report(blocking(|| storage.set_folded_group(&key, folded))).is_some() {
            let groups = Arc::make_mut(&mut self.state.folded_groups);
            if folded {
                groups.insert(key);
            } else {
                groups.remove(&key);
            }
        }
    }

    fn remove_project(&mut self, id: leon_domain::ProjectId) {
        let storage = Arc::clone(&self.ports.storage);
        if Self::report(blocking(|| storage.remove_project(id))).is_none() {
            return;
        }
        let flows: Vec<FlowId> = self
            .state
            .flows
            .iter()
            .filter(|c| c.project == id)
            .map(|c| c.id)
            .collect();
        for flow in flows {
            self.forget(flow);
        }
        Arc::make_mut(&mut self.state.projects).retain(|p| p.id != id);
        Arc::make_mut(&mut self.state.agents).remove(&id);
        Arc::make_mut(&mut self.state.commands).remove(&id);
        Arc::make_mut(&mut self.state.project_repos).remove(&id);
    }

    fn on_event(&mut self, id: ConversationId, event: Option<BackendEvent>) {
        let Some(event) = event else {
            self.live.remove(&id);
            if self
                .state
                .conversations
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
        id: ConversationId,
        usage: Option<leon_domain::Usage>,
        error: Option<String>,
    ) {
        let Some(s) = self.state.conversations.get_mut(&id) else {
            return;
        };
        let partial = s.transcript.as_mut().and_then(|t| t.partial.take());
        let pending: Vec<usize> = s.pending_tools.drain().map(|(_, i)| i).collect();
        let flow = s.conversation.flow;
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
        self.refresh_git(flow);
        // Not an answer for the user: one for another agent, or a turn its
        // agent's work goes on after (NAV-4).
        if self.after_turn(id, failed) || (!failed && self.goes_on(id)) {
            self.touch(flow, false);
        } else {
            self.answered(id, flow, failed);
        }
    }

    /// The agent finished or needs the user: mark unread and notify when the
    /// flow is not on screen (NAV-4, NOTIF-1).
    pub(crate) fn answered(&mut self, id: ConversationId, flow: FlowId, failed: bool) {
        let unread = !self.on_screen(id);
        if let Some(s) = self.state.conversations.get_mut(&id).filter(|_| unread) {
            s.conversation.unread = true;
            self.save_conversation(id);
        }
        self.touch(flow, unread);
        if !unread {
            return;
        }
        let agent = self
            .state
            .conversations
            .get(&id)
            .and_then(|s| s.conversation.settings.agent.clone())
            .unwrap_or_else(|| "The agent".to_owned());
        let title = self
            .state
            .flows
            .iter()
            .find(|c| c.id == flow)
            .map_or_default(|c| c.title.clone());
        let headline = if failed {
            format!("{agent} stopped with an error")
        } else {
            format!("{agent} answered")
        };
        self.state.notify(flow, headline, title);
    }

    pub(crate) fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }

    pub(crate) fn record_entry(&mut self, id: ConversationId, entry: Entry) -> Option<usize> {
        self.record(id, entry)
    }

    pub(crate) fn save_conversation(&mut self, id: ConversationId) {
        let Some(saved) = self
            .state
            .conversations
            .get(&id)
            .map(|s| s.conversation.clone())
        else {
            return;
        };
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.update_conversation(&saved)));
    }

    pub(crate) fn save_entry(&mut self, id: ConversationId, entry: &TimedEntry) {
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.append_entry(id, entry)));
    }
}

/// The message for a new CLI session that must know the flow so far.
fn with_context(s: &ConversationState, text: &str) -> String {
    let mut lines = Vec::new();
    for e in s
        .transcript
        .as_ref()
        .map_or_default(|t| t.entries.as_slice())
    {
        match &e.entry {
            Entry::User { text } => lines.push(format!("User: {text}")),
            Entry::FromAgent { from, text, .. } | Entry::ForYou { from, text } => {
                lines.push(format!("Conversation {}: {text}", from.0));
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

fn describe_change(old: &ConversationSettings, new: &ConversationSettings) -> String {
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
    format!("From your next message: {}.", parts.join(", "))
}
