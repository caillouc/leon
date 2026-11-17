//! Traits the adapters implement. Tests use fakes.

use std::path::{Path, PathBuf};

use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, Conversation, ConversationId, FileDiff, Issue,
    Project, ProjectId, PullRequest, SessionSettings, TimedEntry, Usage,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// A port call that failed. Says what Leon was doing and why it failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{context}: {message}")]
pub struct PortError {
    /// What Leon was doing, e.g. "listing projects".
    pub context: &'static str,
    /// The underlying error, as text.
    pub message: String,
}

impl PortError {
    /// Builds an error from any displayable cause.
    pub fn new(context: &'static str, cause: impl std::fmt::Display) -> Self {
        Self {
            context,
            message: cause.to_string(),
        }
    }
}

/// Leon's own data (P5).
///
/// Implementations may block (disk I/O): the core calls them off the async
/// worker threads.
pub trait Storage: Send + Sync + 'static {
    /// All projects, sorted by name.
    fn list_projects(&self) -> Result<Vec<Project>, PortError>;
    /// Registers a folder as a project; returns the existing one if the path
    /// is already registered.
    fn add_project(&self, name: &str, path: &Path, now: i64) -> Result<Project, PortError>;
    /// Every conversation, most recent first.
    fn list_conversations(&self) -> Result<Vec<Conversation>, PortError>;
    /// Every agent session, oldest first.
    fn list_sessions(&self) -> Result<Vec<AgentSession>, PortError>;
    /// Creates a conversation.
    fn create_conversation(
        &self,
        project: ProjectId,
        title: &str,
        now: i64,
    ) -> Result<Conversation, PortError>;
    /// Saves title, pin, archive, unread and last activity.
    fn update_conversation(&self, conversation: &Conversation) -> Result<(), PortError>;
    /// Deletes a conversation, its sessions and their transcripts.
    fn delete_conversation(&self, id: ConversationId) -> Result<(), PortError>;
    /// Creates an agent session.
    fn create_session(&self, new: &NewSession) -> Result<AgentSession, PortError>;
    /// Saves settings, working directory and CLI session id.
    fn update_session(&self, session: &AgentSession) -> Result<(), PortError>;
    /// Appends one transcript entry.
    fn append_entry(&self, session: AgentSessionId, entry: &TimedEntry) -> Result<(), PortError>;
    /// The whole transcript of a session, oldest first.
    fn entries(&self, session: AgentSessionId) -> Result<Vec<TimedEntry>, PortError>;
}

/// What is needed to create an agent session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    /// Conversation it joins.
    pub conversation: ConversationId,
    /// Session that started it; `None` for the user.
    pub started_by: Option<AgentSessionId>,
    /// Backend, agent, model, effort, permission.
    pub settings: SessionSettings,
    /// Where the CLI runs.
    pub working_dir: PathBuf,
}

/// An agent definition the user can talk to (CONV-2), from the user's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDefinition {
    /// Name, as the CLIs know it.
    pub name: String,
    /// One-line description from the frontmatter.
    pub description: String,
    /// Body of the file: the agent's instructions.
    pub instructions: String,
}

/// Where agent definitions come from (EXT-1, P3).
pub trait Catalog: Send + Sync + 'static {
    /// Agents available in `project`, user-level and project-level, sorted by
    /// name.
    fn agents(&self, project: &Path) -> Vec<AgentDefinition>;
}

/// What a backend needs to start a CLI session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSpec {
    /// Backend, agent, model, effort, permission.
    pub settings: SessionSettings,
    /// The agent's definition, when one is chosen.
    pub agent: Option<AgentDefinition>,
    /// Where the CLI runs.
    pub working_dir: PathBuf,
    /// The CLI's session id to resume, if any.
    pub resume: Option<String>,
}

/// Something the CLI did, normalized across backends.
#[derive(Debug, Clone, PartialEq)]
pub enum BackendEvent {
    /// The CLI session exists; its id resumes it later.
    Ready {
        /// The CLI's own session id.
        cli_session: String,
    },
    /// More text of the agent's current message.
    TextDelta(String),
    /// The agent's complete message.
    Text(String),
    /// The agent called a tool.
    Tool {
        /// CLI's id for the call.
        id: String,
        /// What it did.
        name: String,
        /// Short detail.
        detail: String,
    },
    /// A tool call finished.
    ToolDone {
        /// CLI's id for the call.
        id: String,
        /// Output, possibly shortened.
        output: String,
        /// The tool failed.
        failed: bool,
    },
    /// Subagents run by the CLI inside this session (CONV-13).
    Subagents {
        /// Started so far in this session.
        started: u32,
        /// Still running.
        running: u32,
    },
    /// The CLI asks permission or asks the user something (CONV-9).
    Question {
        /// Id to answer with.
        id: String,
        /// The question.
        title: String,
        /// Extra detail (a command, a path).
        detail: String,
        /// Possible answers, first one recommended.
        options: Vec<String>,
    },
    /// The turn ended.
    TurnEnded {
        /// The user stopped it.
        stopped: bool,
        /// Usage reported for the turn.
        usage: Option<Usage>,
    },
    /// The CLI failed; the session may be resumed with a new message.
    Failed(String),
}

/// Something the core asks a running CLI session to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendCommand {
    /// Send a user message (starts a turn).
    Send(String),
    /// Answer a [`BackendEvent::Question`].
    Answer {
        /// Question id.
        id: String,
        /// Chosen option, or free text.
        answer: String,
    },
    /// Stop the running turn (CONV-6).
    Interrupt,
}

/// A running CLI session: send commands, receive events. Dropping the
/// command sender stops the CLI.
#[derive(Debug)]
pub struct BackendChannel {
    /// Commands to the CLI.
    pub commands: UnboundedSender<BackendCommand>,
    /// Events from the CLI.
    pub events: UnboundedReceiver<BackendEvent>,
}

/// Models, efforts and permission modes a backend offers (pickers, CONV-4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackendOptions {
    /// Model ids; the first is the default.
    pub models: Vec<String>,
    /// Effort levels.
    pub efforts: Vec<String>,
    /// Permission modes; the first is "auto".
    pub permissions: Vec<String>,
}

/// Starts CLI sessions. Must be called inside the core's tokio runtime.
pub trait Backends: Send + Sync + 'static {
    /// Starts a CLI session.
    fn start(&self, spec: SessionSpec) -> Result<BackendChannel, PortError>;
    /// What the pickers offer for `backend`.
    fn options(&self, backend: BackendKind) -> BackendOptions;
}

/// The `PATH` adapters start programs with: the user's login-shell `PATH`,
/// looked up once on first use, or `None` to keep the inherited one. An app
/// started from the Finder inherits a minimal `PATH` that misses
/// `/opt/homebrew/bin` or `~/.local/bin`.
pub type SearchPath = fn() -> Option<&'static std::ffi::OsString>;

/// No `PATH` change: the inherited `PATH` (tests, terminals).
pub fn inherited_path() -> Option<&'static std::ffi::OsString> {
    None
}

/// Reads a working directory's git state (GIT-1..3, CONV-14). Read-only:
/// the app never changes a repository (D4). Calls may block.
pub trait Git: Send + Sync + 'static {
    /// Current branch name; `None` outside a repository or on a detached HEAD.
    fn branch(&self, dir: &Path) -> Option<String>;
    /// The branch to compare with when none was recorded: the remote's
    /// default branch, e.g. `origin/main`.
    fn default_base(&self, dir: &Path) -> Option<String>;
    /// Uncommitted changes: working tree against `HEAD`, untracked files
    /// included (GIT-1, GIT-2).
    fn changes(&self, dir: &Path) -> Result<Vec<FileDiff>, PortError>;
    /// What the branch changed since it left `base`, as a PR shows it:
    /// `merge-base(base, HEAD)..HEAD` (GIT-3).
    fn branch_diff(&self, dir: &Path, base: &str) -> Result<Vec<FileDiff>, PortError>;
}

/// Talks to GitHub with the user's own login (P7). Calls may block.
pub trait GitHub: Send + Sync + 'static {
    /// The pull request whose head is `branch` in the repository at `dir`
    /// (NAV-6, GH-1), open ones first; `None` when there is none.
    fn pull_request(&self, dir: &Path, branch: &str) -> Result<Option<PullRequest>, PortError>;
    /// Open issues assigned to the user (GH-2).
    fn my_issues(&self) -> Result<Vec<Issue>, PortError>;
    /// Open pull requests the user opened (GH-3).
    fn my_pull_requests(&self) -> Result<Vec<PullRequest>, PortError>;
    /// Asks Copilot to review a pull request (GH-5).
    fn request_copilot_review(&self, repo: &str, number: u64) -> Result<(), PortError>;
    /// Posts a comment on a pull request or an issue (GH-6).
    fn comment(&self, repo: &str, number: u64, body: &str) -> Result<(), PortError>;
}
