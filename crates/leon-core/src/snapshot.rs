//! What the UI reads: immutable, cheap to clone (everything big is behind an
//! `Arc`).

use std::collections::HashMap;
use std::sync::Arc;

use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, Conversation, ConversationId, FileDiff, FoldPattern,
    Issue, Project, ProjectId, PullRequest, Settings, Status, TimedEntry,
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
    /// `diff.fold` patterns (GIT-4).
    pub fold: Arc<Vec<FoldPattern>>,
    /// The latest short message to show the user (an action's result), with
    /// a unique id.
    pub flash: Option<(u64, String)>,
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
    /// A question or permission request waiting for the user (CONV-9).
    pub question: Option<Question>,
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
