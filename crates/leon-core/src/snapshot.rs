//! What the UI reads: immutable, cheap to clone (everything big is behind an
//! `Arc`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_domain::{
    BackendKind, Conversation, ConversationId, FileDiff, Flow, FlowId, FoldPattern, GhRef, Issue,
    Project, ProjectId, PullRequest, Settings, Status, TimedEntry,
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
    /// Flows, most recent activity first.
    pub flows: Arc<Vec<FlowView>>,
    /// Every conversation, by id.
    pub conversations: Arc<HashMap<ConversationId, ConversationView>>,
    /// Transcripts loaded so far (those of flows the user opened).
    pub transcripts: HashMap<ConversationId, Arc<TranscriptView>>,
    /// Agents available per project (CONV-2).
    pub agents: Arc<HashMap<ProjectId, Vec<AgentDefinition>>>,
    /// Skills and commands per project, for the `/` list (CONV-11).
    pub commands: Arc<HashMap<ProjectId, Vec<crate::ports::SlashCommand>>>,
    /// The user-level folders of agents and skills, with what each holds
    /// (SETTINGS-1).
    pub folders: Arc<Vec<crate::ports::AgentFolder>>,
    /// The GitHub lists' folded groups, by key (GH-2, GH-3).
    pub folded_groups: Arc<std::collections::HashSet<String>>,
    /// What each backend's pickers offer (CONV-4).
    pub options: Arc<HashMap<BackendKind, BackendOptions>>,
    /// Git state of each folder of a flow (GIT-*, CONV-14, CTX-2).
    pub git: Arc<HashMap<PathBuf, GitView>>,
    /// Pull request of each folder's branch (NAV-6, GH-1, CTX-2).
    pub prs: Arc<HashMap<PathBuf, PullRequest>>,
    /// Issues linked to flows, as GitHub last gave them (CTX-2).
    pub linked_issues: Arc<HashMap<GhRef, Issue>>,
    /// Pull requests linked to flows, as GitHub last gave them (CTX-2).
    pub linked_prs: Arc<HashMap<GhRef, PullRequest>>,
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
    /// What each CLI that answered has as MCP servers and plugins, in
    /// `BackendKind::ALL` order, and when the last answer came.
    pub tools: Option<(i64, Arc<Vec<leon_domain::CliConnectors>>)>,
    /// The CLIs still being asked: their columns show that they are loading.
    pub tools_pending: Vec<leon_domain::BackendKind>,
    /// The GitHub repository (`owner/name`) of each project, from its
    /// `origin`: an issue starts a flow in its repository's project.
    pub project_repos: Arc<HashMap<ProjectId, String>>,
    /// Desktop notifications to show (NOTIF-1), oldest first. Each has a
    /// unique id so the UI shows it once.
    pub notifications: Arc<Vec<Notification>>,
}

impl Snapshot {
    /// A flow by id.
    pub fn flow(&self, id: FlowId) -> Option<&FlowView> {
        self.flows.iter().find(|c| c.flow.id == id)
    }

    /// A flow's pull requests, each once: its folders' in folder order,
    /// then those linked to it (CTX-2).
    pub fn prs_of(&self, c: &FlowView) -> Vec<&PullRequest> {
        let mut prs: Vec<&PullRequest> = Vec::new();
        let linked = c.flow.links.iter().filter_map(|l| self.linked_prs.get(l));
        for pr in c
            .folders
            .iter()
            .filter_map(|f| self.prs.get(f))
            .chain(linked)
        {
            if !prs
                .iter()
                .any(|p| p.repo == pr.repo && p.number == pr.number)
            {
                prs.push(pr);
            }
        }
        prs
    }
}

/// A flow with what the left pane shows about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowView {
    /// The stored flow.
    pub flow: Flow,
    /// Status of its conversations combined (NAV-5).
    pub status: Status,
    /// Its conversations, the one the user started first.
    pub conversations: Vec<ConversationId>,
    /// The folders it works in, never empty once it has a conversation (CTX-2).
    pub folders: Vec<PathBuf>,
    /// The issues it is linked to, the one it was started for first
    /// (CTX-2).
    pub issues: Vec<GhRef>,
}

/// An conversation with its live state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationView {
    /// The stored conversation.
    pub conversation: Conversation,
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
    /// The conversations that owe it an answer: working on its request, or
    /// with the request queued (FLOW-1, CONV-16).
    pub waiting_for: Vec<ConversationId>,
    /// Its name, the same for the user and the agents: "coder 2".
    pub name: String,
    /// The folder of its flow it works in (CTX-2), if known.
    pub folder: Option<PathBuf>,
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
    /// Flow it is about.
    pub flow: FlowId,
    /// First line.
    pub title: String,
    /// Second line.
    pub body: String,
}

/// Git state of a folder.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitView {
    /// `owner/name` of its GitHub repository, from `origin` (CTX-2).
    pub repo: Option<String>,
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
