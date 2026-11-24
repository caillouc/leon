//! Traits the adapters implement. Tests use fakes.

use std::path::{Path, PathBuf};

use leon_domain::{
    BackendKind, Conversation, ConversationId, ConversationSettings, DoctorCheck, DoctorSection,
    FileDiff, Flow, FlowId, Issue, McpServer, Project, ProjectId, PullRequest, TimedEntry, Todo,
    TodoId, Usage,
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
    /// Folds or unfolds a project's flows in the left pane.
    fn set_project_collapsed(&self, id: ProjectId, collapsed: bool) -> Result<(), PortError>;
    /// Keeps the projects in `order`, the left pane's (NAV-2).
    fn set_project_order(&self, order: &[ProjectId]) -> Result<(), PortError>;
    /// The GitHub lists' folded groups, by key (GH-2, GH-3).
    fn folded_groups(&self) -> Result<Vec<String>, PortError>;
    /// Keeps the To do page's flow groups in `order` (TODO-3); other flows
    /// keep their place there.
    fn set_todo_order(&self, order: &[FlowId]) -> Result<(), PortError>;
    /// Every flow's to-dos, in the order they were added (TODO-1).
    fn todos(&self) -> Result<Vec<Todo>, PortError>;
    /// Adds a to-do to flow `flow`, its `(when, what)`, by the agent of
    /// conversation `by` or the user (`None`).
    fn add_todo(
        &self,
        flow: FlowId,
        when_what: (&str, &str),
        by: Option<ConversationId>,
        now: i64,
    ) -> Result<Todo, PortError>;
    /// Marks a to-do done, or open again.
    fn set_todo_done(&self, id: TodoId, done: bool) -> Result<(), PortError>;
    /// Removes a to-do for good.
    fn remove_todo(&self, id: TodoId) -> Result<(), PortError>;
    /// Changes a to-do's `(when, what)`.
    fn edit_todo(&self, id: TodoId, when_what: (&str, &str)) -> Result<(), PortError>;
    /// Folds or unfolds a GitHub list's group.
    fn set_folded_group(&self, key: &str, folded: bool) -> Result<(), PortError>;
    /// Removes a project from Leon with its flows, their conversations
    /// and transcripts. The folder stays on disk.
    fn remove_project(&self, id: ProjectId) -> Result<(), PortError>;
    /// Every flow, most recent first.
    fn list_flows(&self) -> Result<Vec<Flow>, PortError>;
    /// Every conversation, oldest first.
    fn list_conversations(&self) -> Result<Vec<Conversation>, PortError>;
    /// Creates a flow.
    fn create_flow(&self, project: ProjectId, title: &str, now: i64) -> Result<Flow, PortError>;
    /// Saves title, pin, archive, unread and last activity.
    fn update_flow(&self, flow: &Flow) -> Result<(), PortError>;
    /// Deletes a flow, its conversations and their transcripts.
    fn delete_flow(&self, id: FlowId) -> Result<(), PortError>;
    /// Creates an conversation.
    fn create_conversation(&self, new: &NewConversation) -> Result<Conversation, PortError>;
    /// Saves settings, working directory and CLI session id.
    fn update_conversation(&self, conversation: &Conversation) -> Result<(), PortError>;
    /// Appends one transcript entry.
    fn append_entry(
        &self,
        conversation: ConversationId,
        entry: &TimedEntry,
    ) -> Result<(), PortError>;
    /// The whole transcript of a conversation, oldest first.
    fn entries(&self, conversation: ConversationId) -> Result<Vec<TimedEntry>, PortError>;
    /// Replaces a conversation's whole transcript (CONV-7: going back).
    fn replace_entries(
        &self,
        conversation: ConversationId,
        entries: &[TimedEntry],
    ) -> Result<(), PortError>;
}

/// What is needed to create an conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConversation {
    /// Flow it joins.
    pub flow: FlowId,
    /// Conversation that started it; `None` for the user.
    pub started_by: Option<ConversationId>,
    /// Backend, agent, model, effort, permission.
    pub settings: ConversationSettings,
    /// Where the CLI runs.
    pub working_dir: PathBuf,
}

/// An agent definition the user can talk to (CONV-2), from the user's files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentDefinition {
    /// Name, as the CLIs know it.
    pub name: String,
    /// One-line description from the frontmatter.
    pub description: String,
    /// Body of the file: the agent's instructions.
    pub instructions: String,
    /// Its `model`, as written: it picks the CLI (EXT-6).
    pub model: Option<String>,
    /// What its `tools` allow (EXT-7); `None`: everything its CLI has.
    pub tools: Option<Vec<leon_domain::Capability>>,
}

/// Instructions every agent gets, in every CLI (EXT-8): `~/.agents/AGENTS.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedInstructions {
    /// The file.
    pub file: PathBuf,
    /// Its text.
    pub text: String,
}

/// A folder Leon reads agents and skills from, with what it holds
/// (SETTINGS-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFolder {
    /// As shown: `~/.claude`, or as the user wrote it in the config.
    pub label: String,
    /// The user added it (`[sources] folders`); else Leon reads it anyway.
    pub added: bool,
    /// Agent files in it.
    pub agents: usize,
    /// Skills and commands in it.
    pub skills: usize,
}

/// A skill Leon hands to every CLI (EXT-5): from `.agents/skills/` (the
/// user's or the project's) or a source folder's `skills/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFile {
    /// Its name.
    pub name: String,
    /// Its `SKILL.md`, or the `<name>.md` file that is the whole skill.
    pub file: PathBuf,
    /// The skill's folder, when it is one (`<name>/SKILL.md` and its files).
    pub folder: Option<PathBuf>,
    /// From a source folder, which Claude Code loads itself (I10).
    pub from_source: bool,
}

/// A skill or a command the user can type after `/` (CONV-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    /// Name, typed as `/name`.
    pub name: String,
    /// One-line description.
    pub description: String,
    /// The arguments it takes, from its `argument-hint` field, e.g.
    /// `[environment] [version]`.
    pub argument_hint: Option<String>,
}

/// A folder the user registered in the plugin layout (P3, EXT-2): `agents/`,
/// `skills/`, `commands/`, `.mcp.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The folder.
    pub dir: PathBuf,
    /// Its `.mcp.json` servers that run over stdio.
    pub mcp_servers: Vec<McpServer>,
}

/// Leon's shared MCP list (EXT-2, I14): servers given to every CLI.
pub trait SharedTools: Send + Sync + 'static {
    /// Where the list lives.
    fn path(&self) -> &Path;
    /// The servers, by name.
    fn list(&self) -> Result<Vec<McpServer>, PortError>;
    /// Adds a server, or replaces one of the same name.
    fn add(&self, server: &McpServer) -> Result<(), PortError>;
    /// Removes a server.
    fn remove(&self, name: &str) -> Result<(), PortError>;
}

/// The user's config file (P9), which the Settings view writes.
pub trait ConfigFile: Send + Sync + 'static {
    /// Where it is; `None` when the user has no home folder.
    fn path(&self) -> Option<&Path>;
    /// Writes `settings` into it, keeping the rest of the file.
    fn save(&self, settings: &leon_domain::Settings) -> Result<(), PortError>;
}

/// Where agent definitions come from (EXT-1, P3).
pub trait Catalog: Send + Sync + 'static {
    /// Agents available in `project`, user-level and project-level, sorted by
    /// name.
    fn agents(&self, project: &Path) -> Vec<AgentDefinition>;
    /// Skills and commands for the `/` list of a project (CONV-11).
    fn commands(&self, project: &Path) -> Vec<SlashCommand>;
    /// The skills every CLI of a project gets (EXT-5), one per name: the
    /// user's, then the source folders', then the project's, a later one
    /// hiding an earlier one.
    fn skills(&self, project: &Path) -> Vec<SkillFile>;
    /// The user's source folders, with their MCP servers (EXT-2).
    fn sources(&self) -> Vec<Source>;
    /// The user-level folders Leon reads (the ones it finds, then the ones
    /// the user added), with what each holds.
    fn folders(&self) -> Vec<AgentFolder>;
    /// `~/.agents/AGENTS.md`, when it has text (EXT-8).
    fn instructions(&self) -> Option<SharedInstructions>;
    /// Doctor: the source folders and what was found in them.
    fn doctor(&self) -> Vec<DoctorCheck>;
}

/// What a backend needs to start a CLI session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSpec {
    /// The conversation it runs for: its holder is found again by it (I24).
    pub conversation: leon_domain::ConversationId,
    /// Backend, agent, model, effort, permission.
    pub settings: ConversationSettings,
    /// The agent's definition, when one is chosen.
    pub agent: Option<AgentDefinition>,
    /// Where the CLI runs.
    pub working_dir: PathBuf,
    /// The CLI's session id to resume, if any.
    pub resume: Option<String>,
    /// MCP servers to add to the conversation: the app's own tools (P10).
    pub mcp_servers: Vec<McpServer>,
    /// The user's source folders (EXT-2): each CLI loads what it can.
    pub sources: Vec<Source>,
    /// The generic skills every CLI should see (EXT-5).
    pub skills: Vec<SkillFile>,
    /// What every agent is told, in every CLI (EXT-8).
    pub instructions: Option<SharedInstructions>,
    /// What the app tells every agent about its tools (CTX-3), when it
    /// gives them.
    pub app_instructions: Option<&'static str>,
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
    /// Files a tool call changes, as the CLI gave them: absolute, or
    /// relative to the conversation's folder (CTX-2).
    FilesChanged(Vec<PathBuf>),
    /// A tool call finished.
    ToolDone {
        /// CLI's id for the call.
        id: String,
        /// Output, possibly shortened.
        output: String,
        /// The tool failed.
        failed: bool,
    },
    /// A reply the user may want to send next (CONV-10).
    Suggestion(String),
    /// Something the user should know that is not the agent's answer, e.g.
    /// the CLI could not resume its conversation and started a new one.
    Notice(String),
    /// The agent's plan, whole, each time it changes (CONV-14).
    Plan(Vec<leon_domain::PlanStep>),
    /// Subagents run by the CLI inside this conversation (CONV-13).
    Subagents {
        /// Started so far in this conversation.
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
    /// The CLI failed; the conversation may be resumed with a new message.
    Failed(String),
    /// How much of the user's plan limits the CLI says is used, each window
    /// shortest first (DOCTOR-2).
    Limits(Vec<leon_domain::LimitWindow>),
}

/// The image files Leon sends (CONV-18): extension, MIME type.
const IMAGE_TYPES: [(&str, &str); 5] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

/// An image file's MIME type by its extension, in any case; `None` for any
/// other file (CONV-18).
pub fn image_mime(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    IMAGE_TYPES
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(ext))
        .map(|(_, mime)| *mime)
}

/// An image the user put in a message (CONV-18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// Pasted: its bytes and its file extension (`png`, `jpg`, `gif`, `webp`).
    Pasted {
        /// The image file's bytes.
        bytes: std::sync::Arc<[u8]>,
        /// Its file extension.
        ext: &'static str,
    },
    /// A file dropped on the message box.
    File(PathBuf),
}

/// A message to a CLI: its text, and the images saved for it, in order
/// (CONV-18, I27).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Prompt {
    /// The text, "[Image #N]" where each image was put.
    pub text: String,
    /// Each image's file in Leon's data folder.
    pub images: Vec<PathBuf>,
}

impl From<String> for Prompt {
    fn from(text: String) -> Self {
        Self {
            text,
            images: Vec::new(),
        }
    }
}

impl From<&str> for Prompt {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}

/// Something the core asks a running CLI session to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendCommand {
    /// Send a user message (starts a turn).
    Send(Prompt),
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

/// A model a backend offers: the id given to the CLI, the name shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelOption {
    /// Passed to the CLI, e.g. `claude-opus-5-5`.
    pub id: String,
    /// Shown in the pickers, e.g. `Opus 5.5`.
    pub label: String,
    /// Other names the CLI takes for it, e.g. `opus` (EXT-6).
    pub aliases: Vec<String>,
}

/// Models, efforts and permission modes a backend offers (pickers, CONV-4).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BackendOptions {
    /// Models; the first is used when none is chosen.
    pub models: Vec<ModelOption>,
    /// Effort levels.
    pub efforts: Vec<String>,
    /// Effort used when none is chosen.
    pub default_effort: Option<String>,
    /// Permission modes; the first is "auto".
    pub permissions: Vec<String>,
}

impl BackendOptions {
    /// Fills the model and effort left unchosen with the defaults, so every
    /// conversation runs with explicit settings and shows what it uses.
    pub fn complete(&self, settings: &mut ConversationSettings) {
        if settings.model.is_none() {
            settings.model = self.models.first().map(|m| m.id.clone());
        }
        if settings.effort.is_none() {
            settings.effort.clone_from(&self.default_effort);
        }
    }

    /// Every name this CLI takes for a model, with the id it stands for.
    pub fn model_names(&self) -> Vec<(String, String)> {
        self.models
            .iter()
            .flat_map(|m| {
                std::iter::once(&m.id)
                    .chain(&m.aliases)
                    .map(|name| (name.clone(), m.id.clone()))
            })
            .collect()
    }

    /// The name shown for a model id (the id itself when unknown).
    pub fn model_label(&self, id: &str) -> String {
        self.models
            .iter()
            .find(|m| m.id == id)
            .map_or_else(|| id.to_owned(), |m| m.label.clone())
    }
}

/// A conversation's CLI and that CLI's own id of its session.
pub type CliSession = (BackendKind, String);

/// Starts CLI sessions. Must be called inside the core's tokio runtime.
pub trait Backends: Send + Sync + 'static {
    /// Starts a CLI session.
    fn start(&self, spec: SessionSpec) -> Result<BackendChannel, PortError>;
    /// What the pickers offer for `backend`.
    fn options(&self, backend: BackendKind) -> BackendOptions;
    /// A short title for a flow that starts with `message`, from a
    /// one-off call to `backend` (NAV-9, D1). Blocks until the CLI answers.
    fn title(&self, backend: BackendKind, message: &str) -> Result<String, PortError>;
    /// Doctor: one section per CLI (installed, version, logged in). Blocks.
    fn doctor(&self) -> Vec<DoctorSection>;
    /// Connectors view (TOOLS-1): `backend`'s plugins, MCP servers and
    /// account connectors, with their state. Blocks; can take seconds.
    fn connectors(&self, backend: BackendKind) -> leon_domain::CliConnectors;
    /// Removes what conversation `id` leaves once deleted (NAV-8): its CLI,
    /// stopped, Leon's log of it, and `session` (its CLI and that CLI's
    /// session id) where the CLI keeps sessions in files alone. Blocks
    /// briefly.
    fn discard(&self, id: ConversationId, session: Option<CliSession>) {
        let _ = (id, session);
    }
    /// Saves an image of a message to conversation `id` with the
    /// conversation's files, and returns its path (CONV-18, I27). Blocks
    /// briefly.
    fn save_image(&self, id: ConversationId, image: &Image) -> Result<PathBuf, PortError> {
        let _ = (id, image);
        Err(PortError::new(
            "saving an image",
            "images are not kept here",
        ))
    }
    /// The conversations whose CLI may still run from before Leon started
    /// (I24).
    fn held(&self) -> Vec<leon_domain::ConversationId> {
        Vec::new()
    }
    /// The CLI of `spec`'s conversation, still running from before Leon
    /// started (I24); `None` when there is none.
    fn attach(&self, spec: SessionSpec) -> Option<Attached> {
        let _ = spec;
        None
    }
}

/// A CLI found again when Leon starts (I24).
#[derive(Debug)]
pub struct Attached {
    /// Its events, from the start of the turn it was in when Leon quit.
    pub channel: BackendChannel,
    /// It was in a turn: its events replay that turn.
    pub busy: bool,
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

/// Reads a working directory's git state (GIT-1..3, CONV-14). The app never
/// changes a repository (D4): the one write is [`Git::fetch`], which only
/// brings a remote-tracking branch up to date (I22). Calls may block.
pub trait Git: Send + Sync + 'static {
    /// Current branch name; `None` outside a repository or on a detached HEAD.
    fn branch(&self, dir: &Path) -> Option<String>;
    /// `dir` is a repository's own checkout, not one of its linked
    /// worktrees (CTX-2); `false` outside a repository.
    fn is_main_checkout(&self, dir: &Path) -> bool;
    /// The branch to compare with when none was recorded: the remote's
    /// default branch, e.g. `origin/main`.
    fn default_base(&self, dir: &Path) -> Option<String>;
    /// Uncommitted changes: working tree against `HEAD`, untracked files
    /// included (GIT-1, GIT-2).
    fn changes(&self, dir: &Path) -> Result<Vec<FileDiff>, PortError>;
    /// What the branch changed since it left `base`, as a PR shows it:
    /// `merge-base(base, HEAD)..HEAD` (GIT-3).
    fn branch_diff(&self, dir: &Path, base: &str) -> Result<Vec<FileDiff>, PortError>;
    /// Brings `origin/<branch>` up to date from the remote, without a
    /// prompt; files, local branches and `FETCH_HEAD` stay as they are
    /// (GIT-3, I22).
    fn fetch(&self, dir: &Path, branch: &str) -> Result<(), PortError>;
    /// Doctor: git installed and its version. Blocks.
    fn doctor(&self) -> DoctorSection;
    /// `owner/name` of the GitHub repository `origin` points to, if any.
    fn github_repo(&self, dir: &Path) -> Option<String>;
    /// Root of the working tree — a repository or one of its worktrees —
    /// holding `path`, a file or a folder that may be gone (deleted);
    /// `None` outside a repository (CTX-2).
    fn working_tree(&self, path: &Path) -> Option<PathBuf>;
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
    /// One pull request with its checks, reviewers and comments (GH-3).
    fn pull_request_by_number(&self, repo: &str, number: u64) -> Result<PullRequest, PortError>;
    /// A pull request's changes as GitHub's "Files changed" shows them
    /// (GH-3).
    fn pull_request_diff(
        &self,
        repo: &str,
        number: u64,
    ) -> Result<Vec<leon_domain::FileDiff>, PortError>;
    /// One issue with its board fields, assignees, body and comments (GH-2).
    fn issue(&self, repo: &str, number: u64) -> Result<Issue, PortError>;
    /// Asks Copilot to review a pull request (GH-5).
    fn request_copilot_review(&self, repo: &str, number: u64) -> Result<(), PortError>;
    /// Posts a comment on a pull request or an issue (GH-6).
    fn comment(&self, repo: &str, number: u64, body: &str) -> Result<(), PortError>;
    /// Marks a pull request as a draft (`true`) or ready for review (GH-7).
    fn set_draft(&self, repo: &str, number: u64, draft: bool) -> Result<(), PortError>;
    /// Answers a review thread, by its node id (GH-8).
    fn reply_to_thread(&self, thread: &str, body: &str) -> Result<(), PortError>;
    /// Resolves a review thread, by its node id (GH-8).
    fn resolve_thread(&self, thread: &str) -> Result<(), PortError>;
    /// The latest release of `repo` (`owner/name`), drafts and
    /// pre-releases left out (DOCTOR-3).
    fn latest_release(&self, repo: &str) -> Result<leon_domain::Release, PortError>;
    /// Downloads file `name` of release `version` of `repo` into `dir` and
    /// returns its path (DOCTOR-3). Blocks.
    fn download_release(
        &self,
        repo: &str,
        version: &str,
        name: &str,
        dir: &Path,
    ) -> Result<PathBuf, PortError>;
    /// Doctor: `gh` installed, its version, logged in. Blocks.
    fn doctor(&self) -> DoctorSection;
}

/// Leon putting a newer release of itself in its own place (DOCTOR-3, I26).
pub trait Updater: Send + Sync + 'static {
    /// The GitHub repository Leon's releases come from, `owner/name`.
    fn repo(&self) -> &str;
    /// The version running.
    fn version(&self) -> &str;
    /// Whether this Leon can put a release in its own place: on macOS, run
    /// from its app bundle.
    fn can_install(&self) -> bool;
    /// An empty folder to download a release into.
    fn downloads(&self) -> Result<PathBuf, PortError>;
    /// Checks `file`, the release's download, is Leon's own, and puts it in
    /// place of the Leon running. Blocks.
    fn install(&self, file: &Path, release: &leon_domain::Release) -> Result<(), PortError>;
    /// Starts Leon again once this process has ended.
    fn relaunch(&self) -> Result<(), PortError>;
}

/// A call to one of the tools the app gives every conversation (P10, P15),
/// made through the app's MCP server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppToolCall {
    /// Start another agent in a new conversation of the same flow
    /// (FLOW-1). Returns at once; the answer comes back as a message.
    StartAgent {
        /// Agent name, as in its agent file.
        agent: String,
        /// First message to it.
        prompt: String,
        /// Where it runs; the caller's working directory when `None`.
        working_directory: Option<PathBuf>,
        /// Started for the user: its answers go to the user, who reviews
        /// and continues it, not back to the caller.
        for_user: bool,
    },
    /// Send a follow-up to another conversation of the flow
    /// (FLOW-2). Returns at once; the answer comes back as a message.
    MessageAgent {
        /// Target conversation: its name as the user sees it ("coder 2") or
        /// its id.
        conversation: String,
        /// Message text.
        text: String,
    },
    /// Rename the flow.
    RenameFlow {
        /// New title.
        title: String,
    },
    /// The calling conversation moved to another folder, e.g. a worktree (D3).
    SetWorkingDirectory {
        /// Absolute path.
        path: PathBuf,
    },
    /// Link an issue or a pull request to the flow (CTX-2).
    LinkGithub {
        /// Its GitHub link: `https://github.com/owner/name/issues/12` or
        /// `…/pull/12`.
        url: String,
    },
    /// Add a to-do for the user to the flow (TODO-1).
    AddTodo {
        /// What it waits for: an event, a date, an answer.
        when: String,
        /// What the user will do then.
        what: String,
    },
    /// List the flow's to-dos (TODO-1).
    ListTodos,
}

/// An [`AppToolCall`] from a conversation, with where to send the result text
/// (or why it failed) for the agent.
#[derive(Debug)]
pub struct AppToolRequest {
    /// The calling conversation.
    pub conversation: ConversationId,
    /// What it asks.
    pub call: AppToolCall,
    /// Result for the agent: `Ok` text or `Err` message.
    pub reply: tokio::sync::oneshot::Sender<Result<String, String>>,
}

/// How a CLI reaches the app's MCP server (P10).
pub trait AppTools: Send + Sync + 'static {
    /// The MCP server to give one conversation; its tool calls arrive as
    /// [`AppToolRequest`]s from that conversation.
    fn server(&self, conversation: ConversationId) -> McpServer;
    /// Doctor: whether agents can reach the app's tools. Blocks.
    fn doctor(&self) -> DoctorCheck;
    /// What every conversation is told about these tools: when to call
    /// them so the app shows the current work (CTX-3).
    fn instructions(&self) -> &'static str;
}

/// Where the app's MCP server sends tool calls to the core.
pub type AppToolSender = tokio::sync::mpsc::UnboundedSender<AppToolRequest>;
