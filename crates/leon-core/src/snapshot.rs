//! What the UI reads: immutable, cheap to clone (everything big is behind an
//! `Arc`).

use std::collections::HashMap;
use std::sync::Arc;

use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, Conversation, ConversationId, FileDiff, FoldPattern,
    GhRef, Issue, Project, ProjectId, PullRequest, Settings, Status, TimedEntry,
};

use crate::ports::{AgentDefinition, BackendOptions};

/// The application state as the UI sees it.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// `false` until the store has been read.
    pub loaded: bool,
    /// The user's settings.
    pub settings: Settings,
    /// Projects, sorted by name (NAV-2).
    pub projects: Arc<Vec<Project>>,
    /// Conversations, most recent activity first.
    pub conversations: Arc<Vec<ConversationView>>,
    /// Every agent session, by id.
    pub sessions: Arc<HashMap<AgentSessionId, SessionView>>,
    /// Transcripts loaded so far (those of conversations the user opened).
    pub transcripts: HashMap<AgentSessionId, Arc<TranscriptView>>,
    /// Agents available per project (CONV-2).
    pub agents: Arc<HashMap<ProjectId, Vec<AgentDefinition>>>,
    /// Skills and commands per project, for the `/` list (CONV-11).
    pub commands: Arc<HashMap<ProjectId, Vec<crate::ports::SlashCommand>>>,
    /// The user-level folders of agents and skills, with what each holds
    /// (SETTINGS-1).
    pub folders: Arc<Vec<crate::ports::AgentFolder>>,
    /// What each backend's pickers offer (CONV-4).
    pub options: Arc<HashMap<BackendKind, BackendOptions>>,
    /// Git state of each conversation's working directory (GIT-*, CONV-14).
    pub git: Arc<HashMap<ConversationId, GitView>>,
    /// Pull request of each conversation's branch (NAV-6, GH-1).
    pub prs: Arc<HashMap<ConversationId, PullRequest>>,
    /// My open issues (GH-2).
    pub my_issues: Arc<Vec<Issue>>,
    /// My open pull requests (GH-3).
    pub my_prs: Arc<Vec<PullRequest>>,
    /// Why the GitHub lists could not be read, if they could not.
    pub github_error: Option<String>,
    /// The issue or pull request whose details are shown (GH-2, GH-3).
    pub detail: Option<Detail>,
    /// `diff.fold` patterns (GIT-4).
    pub fold: Arc<Vec<FoldPattern>>,
    /// The latest short message to show the user (an action's result), with
    /// a unique id.
    pub flash: Option<(u64, String)>,
    /// The last doctor report (sections) and when it was made.
    pub doctor: Option<(i64, Arc<Vec<leon_domain::DoctorSection>>)>,
    /// The doctor's checks are running.
    pub doctor_running: bool,
    /// Leon's shared MCP list (TOOLS-1).
    pub shared_tools: Arc<Vec<leon_domain::McpServer>>,
    /// Where that list is saved.
    pub shared_tools_path: std::path::PathBuf,
    /// The config file the Settings view writes (P9).
    pub settings_path: Option<std::path::PathBuf>,
    /// Why that list could not be read, if it could not.
    pub shared_tools_error: Option<String>,
    /// What each CLI has as MCP servers and plugins, and when it was read.
    pub tools: Option<(i64, Arc<Vec<leon_domain::CliConnectors>>)>,
    /// The CLIs are being asked.
    pub tools_running: bool,
    /// The GitHub repository (`owner/name`) of each project, from its
    /// `origin`: an issue starts a conversation in its repository's project.
    pub project_repos: Arc<HashMap<ProjectId, String>>,
    /// Desktop notifications to show (NOTIF-1), oldest first. Each has a
    /// unique id so the UI shows it once.
    pub notifications: Arc<Vec<Notification>>,
}

impl Snapshot {
    /// A conversation by id.
    pub fn conversation(&self, id: ConversationId) -> Option<&ConversationView> {
        self.conversations.iter().find(|c| c.conversation.id == id)
    }
}

/// A conversation with what the left pane shows about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationView {
    /// The stored conversation.
    pub conversation: Conversation,
    /// Status of its agent sessions combined (NAV-5).
    pub status: Status,
    /// Its agent sessions, the one the user started first.
    pub sessions: Vec<AgentSessionId>,
}

/// An agent session with its live state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    /// The stored session.
    pub session: AgentSession,
    /// Live status.
    pub status: Status,
    /// Subagents run by the CLI: started, still running (CONV-13).
    pub subagents: (u32, u32),
    /// The agent's plan: its steps and where each stands (CONV-14).
    pub plan: Vec<leon_domain::PlanStep>,
    /// A question or permission request waiting for the user (CONV-9).
    pub question: Option<Question>,
    /// Replies the CLI suggests after its answer (CONV-10).
    pub suggestions: Vec<String>,
}

/// A question from the CLI, shown as a card (CONV-9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// Id to answer with.
    pub id: String,
    /// The question.
    pub title: String,
    /// A command or a path it is about.
    pub detail: String,
    /// Possible answers, first one recommended.
    pub options: Vec<String>,
}

/// A transcript: saved entries plus the agent's message being written.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TranscriptView {
    /// Entries, oldest first.
    pub entries: Arc<Vec<TimedEntry>>,
    /// The agent's message still streaming, if any.
    pub partial: Option<String>,
}

/// A desktop notification (NOTIF-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    /// Unique id.
    pub id: u64,
    /// Conversation it is about.
    pub conversation: ConversationId,
    /// First line.
    pub title: String,
    /// Second line.
    pub body: String,
}

/// Git state of a conversation's working directory.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitView {
    /// Current branch (CONV-14).
    pub branch: Option<String>,
    /// Branch the branch diff compares with.
    pub base: Option<String>,
    /// Uncommitted changes (GIT-1, GIT-2).
    pub changes: Arc<Vec<FileDiff>>,
    /// Everything the branch changed since its base (GIT-3).
    pub branch_diff: Arc<Vec<FileDiff>>,
    /// Why git could not be read (not a repository…).
    pub error: Option<String>,
}

/// An issue's or a pull request's details, as read from GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detail {
    /// Being read.
    Loading(GhRef),
    /// An issue, with its body and comments.
    Issue(Box<Issue>),
    /// A pull request, with its checks, reviewers and comments.
    PullRequest(Box<PullRequest>),
    /// Could not be read, and why.
    Failed(GhRef, String),
}

impl Detail {
    /// Which item it is about.
    pub fn item(&self) -> GhRef {
        match self {
            Self::Loading(item) | Self::Failed(item, _) => item.clone(),
            Self::Issue(issue) => GhRef::issue(&issue.repo, issue.number),
            Self::PullRequest(pr) => GhRef::pull_request(&pr.repo, pr.number),
        }
    }
}
