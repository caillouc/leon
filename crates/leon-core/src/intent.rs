//! What the UI asks the core to do.

use std::path::PathBuf;

use leon_domain::{ConversationId, ConversationSettings, FlowId, ProjectId};

/// A request from the UI. The core applies it and publishes a new snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Register a folder as a project.
    AddProject(PathBuf),
    /// Remove a project from Leon with its flows; the folder stays.
    RemoveProject(ProjectId),
    /// Fold (`true`) or unfold a GitHub list's group, kept across restarts.
    FoldGroup(String, bool),
    /// The flow shown in the main pane (`None`: none): loads its
    /// transcripts and git state.
    View(Option<FlowId>),
    /// The conversation whose messages are on screen — its tab, not the
    /// flow's Flow tab (`None`: none). Marks its answer read, and its
    /// flow once no other conversation's answer waits; an answer that
    /// arrives while it is read stays read (NAV-4, FLOW-4).
    Read(Option<ConversationId>),
    /// Leon's window became active (`true`) or went behind another app: an
    /// answer counts as seen only in the active window (NAV-4, NOTIF-1).
    WindowActive(bool),
    /// Start a flow with an agent and send the first message (CONV-2).
    NewFlow {
        /// Project it belongs to.
        project: ProjectId,
        /// Backend, agent, model, effort, permission.
        settings: ConversationSettings,
        /// First message.
        text: String,
        /// The issue it is started for (GH-2), if any.
        issue: Option<leon_domain::GhRef>,
    },
    /// Start another conversation in an existing flow, as the user, and
    /// send its first message (FLOW-7).
    NewConversation {
        /// The flow.
        flow: FlowId,
        /// Backend, agent, model, effort, permission.
        settings: ConversationSettings,
        /// Where its CLI runs: one of the flow's folders.
        working_dir: std::path::PathBuf,
        /// First message.
        text: String,
    },
    /// Send a message to an conversation (CONV-1).
    Send {
        /// Target conversation.
        conversation: ConversationId,
        /// Message text.
        text: String,
    },
    /// Go back to before a message of the user and send `text` instead
    /// (CONV-7, CONV-8): later entries are dropped, the CLI starts a new
    /// conversation carrying the flow up to that point.
    Rewind {
        /// The conversation.
        conversation: ConversationId,
        /// Index of the user's message in the transcript.
        entry: usize,
        /// What to send instead.
        text: String,
    },
    /// Stop the running answer (CONV-6).
    Stop(ConversationId),
    /// Answer a question card (CONV-9).
    Answer {
        /// Conversation that asked.
        conversation: ConversationId,
        /// Question id.
        question: String,
        /// Chosen option or free text.
        answer: String,
    },
    /// Change backend, model, effort or permission (CONV-5).
    ChangeSettings {
        /// Conversation to change.
        conversation: ConversationId,
        /// New settings.
        settings: ConversationSettings,
    },
    /// Fold or unfold a project's flows in the left pane (NAV-2).
    CollapseProject(ProjectId, bool),
    /// Rename a flow (NAV-8).
    Rename(FlowId, String),
    /// Pin or unpin (NAV-3).
    Pin(FlowId, bool),
    /// Archive or unarchive (NAV-8).
    Archive(FlowId, bool),
    /// Mark as unread (NAV-4).
    MarkUnread(FlowId),
    /// Mark one conversation as unread, and so its flow (NAV-4).
    MarkConversationUnread(ConversationId),
    /// Delete a flow (NAV-8).
    Delete(FlowId),
    /// Re-read a flow's git state now.
    RefreshGit(FlowId),
    /// Re-read my issues and pull requests (GH-2, GH-3).
    RefreshGitHub,
    /// Run the doctor's checks again.
    RunDoctor,
    /// Ask each CLI for its own MCP servers and plugins (TOOLS-1).
    RefreshTools,
    /// Add a server to Leon's shared MCP list, or replace one (TOOLS-1).
    AddTool(leon_domain::McpServer),
    /// Remove a server from Leon's shared MCP list (TOOLS-1).
    RemoveTool(String),
    /// Save the user's settings to the config file and use them (SETTINGS-1).
    SaveSettings(leon_domain::Settings),
    /// Ask Copilot to review a pull request (GH-5).
    RequestCopilotReview(leon_domain::GhRef),
    /// Comment on a pull request or an issue (GH-6).
    Comment(leon_domain::GhRef, String),
    /// Mark a pull request as a draft (`true`) or ready for review (GH-7).
    SetDraft(leon_domain::GhRef, bool),
    /// Answer a review thread of a pull request (GH-8).
    ReplyToThread {
        /// The pull request.
        pr: leon_domain::GhRef,
        /// The thread's node id.
        thread: String,
        /// The answer.
        body: String,
    },
    /// Resolve a review thread of a pull request (GH-8).
    ResolveThread {
        /// The pull request.
        pr: leon_domain::GhRef,
        /// The thread's node id.
        thread: String,
    },
    /// Show an issue's or a pull request's details (`None`: close them);
    /// they are read from GitHub in the background.
    ShowDetail(Option<leon_domain::GhRef>),
}
