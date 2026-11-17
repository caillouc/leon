use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{BackendKind, ProjectId};

/// Identifier of a conversation in Leon's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConversationId(pub i64);

/// Identifier of an agent session in Leon's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentSessionId(pub i64);

/// Where a conversation or an agent session stands (NAV-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Status {
    /// No turn running.
    #[default]
    Idle,
    /// A turn is running.
    Running,
    /// Waiting for the user: a question or a permission request.
    NeedsInput,
    /// The last turn failed or the CLI stopped.
    Error,
}

impl Status {
    /// Status of a conversation from the status of its agent sessions:
    /// needs input, then running, then error, then idle.
    pub fn of_conversation(sessions: impl IntoIterator<Item = Self>) -> Self {
        sessions.into_iter().fold(
            Self::Idle,
            |acc, s| if s.rank() > acc.rank() { s } else { acc },
        )
    }

    fn rank(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Error => 1,
            Self::Running => 2,
            Self::NeedsInput => 3,
        }
    }
}

/// One left-pane item: the agent session the user started plus every agent
/// session started from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    /// Store id.
    pub id: ConversationId,
    /// Project it belongs to (NAV-2).
    pub project: ProjectId,
    /// Title shown in the left pane (NAV-9).
    pub title: String,
    /// Pinned to the top of the left pane (NAV-3).
    pub pinned: bool,
    /// Moved to the archive (NAV-8).
    pub archived: bool,
    /// Has something the user has not seen (NAV-4).
    pub unread: bool,
    /// Last activity, Unix milliseconds.
    pub updated_at: i64,
    /// The folder an agent said the conversation moved to, e.g. a worktree
    /// (D3, P10); `None`: the first session's folder.
    pub working_dir: Option<PathBuf>,
    /// The issue it was started for (GH-2, CTX-1), if any.
    pub issue: Option<crate::GhRef>,
}

/// Choices that shape an agent session (CONV-2..4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSettings {
    /// CLI that runs the session.
    pub backend: BackendKind,
    /// Agent name; `None` is the CLI's own agent.
    pub agent: Option<String>,
    /// Model id; `None` lets the CLI choose.
    pub model: Option<String>,
    /// Effort level; `None` lets the CLI choose.
    pub effort: Option<String>,
    /// Permission mode, in the CLI's own words.
    pub permission: String,
}

/// One agent's own CLI conversation inside a conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    /// Store id.
    pub id: AgentSessionId,
    /// Conversation it belongs to.
    pub conversation: ConversationId,
    /// Agent session that started it; `None` when the user did.
    pub started_by: Option<AgentSessionId>,
    /// Backend, agent, model, effort, permission.
    pub settings: SessionSettings,
    /// Where the CLI runs.
    pub working_dir: PathBuf,
    /// The CLI's own session id, once known; used to resume.
    pub cli_session: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::Status;
    use super::Status::{Error, Idle, NeedsInput, Running};

    #[test]
    fn conversation_status_follows_priority() {
        let cases = [
            (vec![], Idle),
            (vec![Idle, Idle], Idle),
            (vec![Idle, Error], Error),
            (vec![Error, Running], Running),
            (vec![Running, NeedsInput, Error], NeedsInput),
        ];
        for (sessions, expected) in cases {
            assert_eq!(
                Status::of_conversation(sessions.clone()),
                expected,
                "{sessions:?}"
            );
        }
    }
}
