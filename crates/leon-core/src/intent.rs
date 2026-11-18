//! What the UI asks the core to do.

use std::path::PathBuf;

use leon_domain::{AgentSessionId, ConversationId, ProjectId, SessionSettings};

/// A request from the UI. The core applies it and publishes a new snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Register a folder as a project.
    AddProject(PathBuf),
    /// The conversation shown in the main pane (`None`: none). Marks it read
    /// and loads its transcripts (NAV-4).
    View(Option<ConversationId>),
    /// Start a conversation with an agent and send the first message (CONV-2).
    NewConversation {
        /// Project it belongs to.
        project: ProjectId,
        /// Backend, agent, model, effort, permission.
        settings: SessionSettings,
        /// First message.
        text: String,
        /// The issue it is started for (GH-2), if any.
        issue: Option<leon_domain::GhRef>,
    },
    /// Send a message to an agent session (CONV-1).
    Send {
        /// Target session.
        session: AgentSessionId,
        /// Message text.
        text: String,
    },
    /// Go back to before a message of the user and send `text` instead
    /// (CONV-7, CONV-8): later entries are dropped, the CLI starts a new
    /// session carrying the conversation up to that point.
    Rewind {
        /// The session.
        session: AgentSessionId,
        /// Index of the user's message in the transcript.
        entry: usize,
        /// What to send instead.
        text: String,
    },
    /// Stop the running answer (CONV-6).
    Stop(AgentSessionId),
    /// Answer a question card (CONV-9).
    Answer {
        /// Session that asked.
        session: AgentSessionId,
        /// Question id.
        question: String,
        /// Chosen option or free text.
        answer: String,
    },
    /// Change backend, model, effort or permission (CONV-5).
    ChangeSettings {
        /// Session to change.
        session: AgentSessionId,
        /// New settings.
        settings: SessionSettings,
    },
    /// Fold or unfold a project's conversations in the left pane (NAV-2).
    CollapseProject(ProjectId, bool),
    /// Rename a conversation (NAV-8).
    Rename(ConversationId, String),
    /// Pin or unpin (NAV-3).
    Pin(ConversationId, bool),
    /// Archive or unarchive (NAV-8).
    Archive(ConversationId, bool),
    /// Mark as unread (NAV-4).
    MarkUnread(ConversationId),
    /// Delete a conversation (NAV-8).
    Delete(ConversationId),
    /// Re-read a conversation's git state now.
    RefreshGit(ConversationId),
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
    /// Show an issue's or a pull request's details (`None`: close them);
    /// they are read from GitHub in the background.
    ShowDetail(Option<leon_domain::GhRef>),
}
