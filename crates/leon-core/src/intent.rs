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
    },
    /// Send a message to an agent session (CONV-1).
    Send {
        /// Target session.
        session: AgentSessionId,
        /// Message text.
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
    /// Ask Copilot to review the conversation's pull request (GH-5).
    RequestCopilotReview(ConversationId),
    /// Comment on the conversation's pull request (GH-6).
    CommentOnPr(ConversationId, String),
}
