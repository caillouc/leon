//! The core's mutable state and how it becomes a [`Snapshot`].

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, Conversation, ConversationId, Project, ProjectId,
    Settings, Status, TimedEntry,
};

use crate::ports::{AgentDefinition, BackendOptions};
use crate::snapshot::{
    ConversationView, Notification, Question, SessionView, Snapshot, TranscriptView,
};

/// Live state of one agent session.
#[derive(Debug)]
pub(crate) struct SessionState {
    pub session: AgentSession,
    pub status: Status,
    pub subagents: (u32, u32),
    pub question: Option<Question>,
    /// `None` until the transcript is loaded from the store.
    pub transcript: Option<TranscriptView>,
    /// Tool calls without a result yet: id → index in the transcript.
    pub pending_tools: HashMap<String, usize>,
    /// The next message must carry the conversation so far (backend changed,
    /// CONV-5).
    pub carry_context: bool,
    /// The session whose request this session is answering (P15); its
    /// answer goes there instead of to the user.
    pub reply_to: Option<AgentSessionId>,
    /// Messages from other sessions waiting for this one to finish its turn.
    pub inbox: VecDeque<Waiting>,
}

/// A message from another agent session, not sent yet.
#[derive(Debug, Clone)]
pub(crate) struct Waiting {
    pub from: AgentSessionId,
    pub text: String,
    /// A request: the answer goes back to `from`.
    pub expects_answer: bool,
}

impl SessionState {
    pub(crate) fn new(session: AgentSession) -> Self {
        Self {
            session,
            status: Status::Idle,
            subagents: (0, 0),
            question: None,
            transcript: None,
            pending_tools: HashMap::new(),
            carry_context: false,
            reply_to: None,
            inbox: VecDeque::new(),
        }
    }

    /// Appends an entry to the loaded transcript; returns its index.
    pub(crate) fn push(&mut self, entry: TimedEntry) -> usize {
        let transcript = self.transcript.get_or_insert_with(TranscriptView::default);
        let entries = Arc::make_mut(&mut transcript.entries);
        entries.push(entry);
        entries.len() - 1
    }
}

#[derive(Debug, Default)]
pub(crate) struct State {
    pub loaded: bool,
    pub settings: Settings,
    pub projects: Arc<Vec<Project>>,
    pub conversations: Vec<Conversation>,
    pub sessions: BTreeMap<AgentSessionId, SessionState>,
    pub agents: Arc<HashMap<ProjectId, Vec<AgentDefinition>>>,
    pub options: Arc<HashMap<BackendKind, BackendOptions>>,
    pub notifications: Arc<Vec<Notification>>,
    pub next_notification: u64,
    pub git: crate::repo::GitMap,
    pub prs: Arc<HashMap<ConversationId, leon_domain::PullRequest>>,
    pub my_issues: Arc<Vec<leon_domain::Issue>>,
    pub my_prs: Arc<Vec<leon_domain::PullRequest>>,
    pub github_error: Option<String>,
    pub fold: Arc<Vec<leon_domain::FoldPattern>>,
    pub flash: Option<(u64, String)>,
}

impl State {
    pub(crate) fn conversation_mut(&mut self, id: ConversationId) -> Option<&mut Conversation> {
        self.conversations.iter_mut().find(|c| c.id == id)
    }

    pub(crate) fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub(crate) fn sessions_of(&self, conversation: ConversationId) -> Vec<AgentSessionId> {
        self.sessions
            .values()
            .filter(|s| s.session.conversation == conversation)
            .map(|s| s.session.id)
            .collect()
    }

    pub(crate) fn notify(&mut self, conversation: ConversationId, title: String, body: String) {
        self.next_notification += 1;
        let list = Arc::make_mut(&mut self.notifications);
        list.push(Notification {
            id: self.next_notification,
            conversation,
            title,
            body,
        });
        if list.len() > 20 {
            list.remove(0);
        }
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        let mut conversations: Vec<ConversationView> = self
            .conversations
            .iter()
            .map(|c| {
                let sessions = self.sessions_of(c.id);
                let status = Status::of_conversation(
                    sessions
                        .iter()
                        .filter_map(|id| self.sessions.get(id))
                        .map(|s| s.status),
                );
                ConversationView {
                    conversation: c.clone(),
                    status,
                    sessions,
                }
            })
            .collect();
        conversations.sort_by_key(|c| std::cmp::Reverse(c.conversation.updated_at));
        let sessions = self
            .sessions
            .values()
            .map(|s| {
                let view = SessionView {
                    session: s.session.clone(),
                    status: s.status,
                    subagents: s.subagents,
                    question: s.question.clone(),
                };
                (s.session.id, view)
            })
            .collect();
        let transcripts = self
            .sessions
            .values()
            .filter_map(|s| {
                s.transcript
                    .as_ref()
                    .map(|t| (s.session.id, Arc::new(t.clone())))
            })
            .collect();
        Snapshot {
            loaded: self.loaded,
            settings: self.settings.clone(),
            projects: Arc::clone(&self.projects),
            conversations: Arc::new(conversations),
            sessions: Arc::new(sessions),
            transcripts,
            agents: Arc::clone(&self.agents),
            options: Arc::clone(&self.options),
            notifications: Arc::clone(&self.notifications),
            git: Arc::clone(&self.git),
            prs: Arc::clone(&self.prs),
            my_issues: Arc::clone(&self.my_issues),
            my_prs: Arc::clone(&self.my_prs),
            github_error: self.github_error.clone(),
            fold: Arc::clone(&self.fold),
            flash: self.flash.clone(),
        }
    }
}

/// A title from the first message (NAV-9): its first line, cut at a word
/// boundary around 60 characters.
pub(crate) fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("New conversation")
        .trim();
    if line.chars().count() <= 60 {
        return line.to_owned();
    }
    let cut: String = line.chars().take(60).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::title_from;

    #[test]
    fn titles_come_from_the_first_line() {
        assert_eq!(
            title_from("\n  Fix the login bug\nmore"),
            "Fix the login bug"
        );
        assert_eq!(title_from(""), "New conversation");
        let long = "Implement the export of every invoice as CSV with the filters the user applied in the table";
        assert_eq!(
            title_from(long),
            "Implement the export of every invoice as CSV with the…"
        );
    }
}
