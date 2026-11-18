use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{BackendKind, ProjectId};

/// Identifier of a flow in Leon's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FlowId(pub i64);

/// Identifier of an conversation in Leon's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConversationId(pub i64);

/// Where a flow or an conversation stands (NAV-5).
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

/// What a flow asks of the user, as the left pane shows it (NAV-5),
/// from least to most pressing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Attention {
    /// Nothing: done and seen.
    Nothing,
    /// An agent is working.
    Working,
    /// Done, not looked at yet: the user reviews what came back.
    Review,
    /// An agent waits for the user's answer or decision.
    NeedsYou,
    /// It stopped with an error.
    Failed,
}

impl Attention {
    /// From the flow's status and whether it is unread.
    pub fn of(status: Status, unread: bool) -> Self {
        match status {
            Status::NeedsInput => Self::NeedsYou,
            Status::Error => Self::Failed,
            Status::Running => Self::Working,
            Status::Idle if unread => Self::Review,
            Status::Idle => Self::Nothing,
        }
    }
}

impl Status {
    /// Status of a flow from the status of its conversations:
    /// needs input, then running, then error, then idle.
    pub fn of_flow(conversations: impl IntoIterator<Item = Self>) -> Self {
        conversations.into_iter().fold(
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

/// One left-pane item: the conversation the user started plus every agent
/// conversation started from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flow {
    /// Store id.
    pub id: FlowId,
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
    /// The folders its agents work in — worktrees, other repositories — in
    /// the order Leon saw them (D3, CTX-2); empty: the first conversation's
    /// folder.
    pub folders: Vec<PathBuf>,
    /// The issue it was started for (GH-2, CTX-1), if any.
    pub issue: Option<crate::GhRef>,
}

/// Choices that shape an conversation (CONV-2..4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationSettings {
    /// CLI that runs the conversation.
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

/// One agent's own CLI flow inside a flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    /// Store id.
    pub id: ConversationId,
    /// Flow it belongs to.
    pub flow: FlowId,
    /// Conversation that started it; `None` when the user did.
    pub started_by: Option<ConversationId>,
    /// Backend, agent, model, effort, permission.
    pub settings: ConversationSettings,
    /// Where the CLI runs.
    pub working_dir: PathBuf,
    /// The CLI's own session id, once known; used to resume.
    pub cli_session: Option<String>,
    /// Its last answer to the user is not read yet (NAV-4, FLOW-4).
    pub unread: bool,
}

#[cfg(test)]
mod tests {
    use super::Status;
    use super::Status::{Error, Idle, NeedsInput, Running};

    #[test]
    fn what_a_flow_asks_of_the_user() {
        use super::Attention;
        let cases = [
            (Idle, false, Attention::Nothing),
            (Idle, true, Attention::Review),
            (Running, true, Attention::Working),
            (NeedsInput, false, Attention::NeedsYou),
            (Error, true, Attention::Failed),
        ];
        for (status, unread, expected) in cases {
            assert_eq!(
                Attention::of(status, unread),
                expected,
                "{status:?} {unread}"
            );
        }
        // A folded project shows its most pressing one.
        let most = [Attention::Working, Attention::Review, Attention::NeedsYou]
            .into_iter()
            .max();
        assert_eq!(most, Some(Attention::NeedsYou));
    }

    #[test]
    fn flow_status_follows_priority() {
        let cases = [
            (vec![], Idle),
            (vec![Idle, Idle], Idle),
            (vec![Idle, Error], Error),
            (vec![Error, Running], Running),
            (vec![Running, NeedsInput, Error], NeedsInput),
        ];
        for (conversations, expected) in cases {
            assert_eq!(
                Status::of_flow(conversations.clone()),
                expected,
                "{conversations:?}"
            );
        }
    }
}
