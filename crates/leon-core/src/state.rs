//! The core's mutable state and how it becomes a [`Snapshot`].

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use leon_domain::{
    BackendKind, Conversation, ConversationId, Flow, FlowId, GhRef, Project, ProjectId, Settings,
    Status, TimedEntry,
};

use crate::ports::{AgentDefinition, BackendOptions};
use crate::snapshot::{
    ConversationView, FlowView, GitView, Notification, Question, Snapshot, TranscriptView,
};

/// Live state of one conversation.
#[derive(Debug)]
pub(crate) struct ConversationState {
    pub conversation: Conversation,
    pub status: Status,
    pub subagents: (u32, u32),
    /// The agent's plan (CONV-14); kept while Leon runs.
    pub plan: Vec<leon_domain::PlanStep>,
    pub question: Option<Question>,
    /// Replies the CLI suggests after its answer (CONV-10).
    pub suggestions: Vec<String>,
    /// `None` until the transcript is loaded from the store.
    pub transcript: Option<TranscriptView>,
    /// Tool calls without a result yet: id → index in the transcript.
    pub pending_tools: HashMap<String, usize>,
    /// The next message must carry the flow so far (backend changed,
    /// CONV-5).
    pub carry_context: bool,
    /// The conversation whose request this conversation is answering (P15); its
    /// answer goes there instead of to the user.
    pub reply_to: Option<ConversationId>,
    /// Messages from other conversations waiting for this one to finish its turn.
    pub inbox: VecDeque<Waiting>,
}

/// A message from another conversation, not sent yet.
#[derive(Debug, Clone)]
pub(crate) struct Waiting {
    pub from: ConversationId,
    pub text: String,
    pub sent: Sent,
}

/// What a message between conversations is (FLOW-1, FLOW-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sent {
    /// A request: its answer goes back to the sender.
    Request,
    /// The answer to a request the receiver sent.
    Answer,
    /// The first message of a conversation started for the user: its
    /// answers go to the user.
    ForUser,
}

impl ConversationState {
    pub(crate) fn new(conversation: Conversation) -> Self {
        Self {
            conversation,
            status: Status::Idle,
            subagents: (0, 0),
            plan: Vec::new(),
            question: None,
            suggestions: Vec::new(),
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
    pub flows: Vec<Flow>,
    pub conversations: BTreeMap<ConversationId, ConversationState>,
    pub agents: Arc<HashMap<ProjectId, Vec<AgentDefinition>>>,
    pub commands: Arc<HashMap<ProjectId, Vec<crate::ports::SlashCommand>>>,
    /// The user's source folders (EXT-2), given to every conversation.
    pub sources: Arc<Vec<crate::ports::Source>>,
    /// The user-level folders of agents and skills (SETTINGS-1).
    pub folders: Arc<Vec<crate::ports::AgentFolder>>,
    /// The GitHub lists' folded groups (GH-2, GH-3).
    pub folded_groups: Arc<std::collections::HashSet<String>>,
    /// Every flow's to-dos (TODO-1).
    pub todos: Arc<Vec<leon_domain::Todo>>,
    pub options: Arc<HashMap<BackendKind, BackendOptions>>,
    pub notifications: Arc<Vec<Notification>>,
    pub next_notification: u64,
    pub git: crate::repo::GitMap,
    /// The pull request of each folder's branch (NAV-6, CTX-2).
    pub prs: Arc<HashMap<PathBuf, leon_domain::PullRequest>>,
    /// The issues and pull requests linked to flows, as GitHub last gave
    /// them (CTX-2).
    pub linked_issues: Arc<HashMap<GhRef, leon_domain::Issue>>,
    pub linked_prs: Arc<HashMap<GhRef, leon_domain::PullRequest>>,
    pub my_issues: Arc<Vec<leon_domain::Issue>>,
    pub my_prs: Arc<Vec<leon_domain::PullRequest>>,
    pub github_error: Option<String>,
    /// The issue or pull request whose details are shown.
    pub detail: Option<crate::Detail>,
    pub pr_diff: Option<crate::PrDiff>,
    pub fold: Arc<Vec<leon_domain::FoldPattern>>,
    pub flash: Option<(u64, String)>,
    /// The last doctor report and when it was made.
    pub doctor: Option<(i64, Arc<Vec<leon_domain::DoctorSection>>)>,
    pub doctor_running: bool,
    /// Leon's shared MCP list (I14), and why it could not be read.
    pub shared: Arc<Vec<leon_domain::McpServer>>,
    pub shared_path: std::path::PathBuf,
    /// The config file Settings writes.
    pub settings_path: Option<std::path::PathBuf>,
    pub shared_error: Option<String>,
    /// What each CLI has as MCP servers and plugins, and when it was read.
    pub tools: Option<(i64, Arc<Vec<leon_domain::CliConnectors>>)>,
    /// The CLIs still being asked for their connectors.
    pub tools_pending: Vec<leon_domain::BackendKind>,
    /// GitHub actions under way (GH-5..8).
    pub acting: Arc<std::collections::HashSet<crate::snapshot::Acting>>,
    /// How much of each CLI's plan limits is used, as it last said (DOCTOR-2).
    pub limits: Arc<Vec<(leon_domain::BackendKind, Vec<leon_domain::LimitWindow>)>>,
    /// The GitHub repository of each project, from its `origin`.
    pub project_repos: Arc<HashMap<ProjectId, String>>,
}

impl State {
    /// A conversation's name, the same for the user and the agents: its
    /// agent (else its CLI), numbered in start order when its flow has
    /// several of that agent: "coder 2".
    pub(crate) fn conversation_name(&self, id: ConversationId) -> String {
        let Some(s) = self.conversations.get(&id) else {
            return format!("conversation {}", id.0);
        };
        let label = |c: &ConversationState| {
            c.conversation
                .settings
                .agent
                .clone()
                .unwrap_or_else(|| c.conversation.settings.backend.display_name().to_owned())
        };
        let name = label(s);
        let same: Vec<ConversationId> = self
            .conversations
            .values()
            .filter(|o| o.conversation.flow == s.conversation.flow && label(o) == name)
            .map(|o| o.conversation.id)
            .collect();
        match same.iter().position(|o| *o == id) {
            Some(ix) if same.len() > 1 => format!("{name} {}", ix + 1),
            _ => name,
        }
    }

    /// The conversation of flow `flow` that `to` names: its name as the user
    /// sees it ("coder 2", any case), its id, or "conversation <id>".
    pub(crate) fn find_conversation(&self, flow: FlowId, to: &str) -> Option<ConversationId> {
        let to = to.trim();
        let id = to
            .strip_prefix("conversation ")
            .unwrap_or(to)
            .trim()
            .parse::<i64>()
            .ok()
            .map(ConversationId);
        self.conversations_of(flow)
            .into_iter()
            .find(|c| Some(*c) == id || self.conversation_name(*c).eq_ignore_ascii_case(to))
    }

    /// A flow's worktrees: its folders but a checkout on its repository's
    /// default branch (CTX-2).
    fn worktrees(&self, flow: FlowId) -> Vec<PathBuf> {
        self.folders_of(flow)
            .into_iter()
            .filter(|f| !self.git.get(f).is_some_and(GitView::on_default_branch))
            .collect()
    }

    /// The folders of a flow the user sees: its worktrees one of its agents
    /// works in now, in the order Leon saw them (CTX-2).
    fn shown_folders(&self, flow: FlowId) -> Vec<PathBuf> {
        let busy: Vec<PathBuf> = self
            .conversations
            .values()
            .filter(|s| s.conversation.flow == flow)
            .filter_map(|s| self.folder_of(s))
            .collect();
        self.worktrees(flow)
            .into_iter()
            .filter(|f| busy.contains(f))
            .collect()
    }

    /// The worktree of its flow a conversation works in: the one it last
    /// worked in, else the one holding its working directory.
    fn folder_of(&self, s: &ConversationState) -> Option<PathBuf> {
        let folders = self.worktrees(s.conversation.flow);
        s.conversation
            .folder
            .clone()
            .filter(|f| folders.contains(f))
            .or_else(|| {
                folders
                    .into_iter()
                    .filter(|f| s.conversation.working_dir.starts_with(f))
                    .max_by_key(|f| f.as_os_str().len())
            })
    }

    /// Every conversation of a flow, by name and id: "planner (1), coder 1 (2)".
    pub(crate) fn roll_call(&self, flow: FlowId) -> String {
        self.conversations_of(flow)
            .into_iter()
            .map(|c| format!("{} ({})", self.conversation_name(c), c.0))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The conversations that owe conversation `id` an answer: working on
    /// its request, or with the request queued until they finish (FLOW-1).
    pub(crate) fn waiting_for(&self, id: ConversationId) -> Vec<ConversationId> {
        self.conversations
            .values()
            .filter(|s| {
                s.reply_to == Some(id)
                    || s.inbox
                        .iter()
                        .any(|w| w.from == id && w.sent == Sent::Request)
            })
            .map(|s| s.conversation.id)
            .collect()
    }

    pub(crate) fn flow_mut(&mut self, id: FlowId) -> Option<&mut Flow> {
        self.flows.iter_mut().find(|c| c.id == id)
    }

    pub(crate) fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    /// The folders a flow works in (CTX-2): those its agents
    /// worked in, else the folder of the conversation the user started.
    pub(crate) fn folders_of(&self, id: FlowId) -> Vec<PathBuf> {
        match self.flows.iter().find(|c| c.id == id) {
            Some(c) if !c.folders.is_empty() => c.folders.clone(),
            Some(_) => self
                .conversations
                .values()
                .find(|s| s.conversation.flow == id && s.conversation.started_by.is_none())
                .map_or_default(|s| vec![s.conversation.working_dir.clone()]),
            None => Vec::new(),
        }
    }

    /// The issues a flow is linked to (CTX-2): the one it was
    /// started for, then those its folders' pull requests close.
    fn issues_of(&self, c: &Flow, folders: &[PathBuf]) -> Vec<GhRef> {
        let mut issues: Vec<GhRef> = c
            .links
            .iter()
            .filter(|l| l.kind == leon_domain::GhKind::Issue)
            .cloned()
            .collect();
        let closed = folders
            .iter()
            .filter_map(|f| self.prs.get(f))
            .flat_map(|pr| &pr.closes);
        for issue in closed {
            if !issues.contains(issue) {
                issues.push(issue.clone());
            }
        }
        issues
    }

    pub(crate) fn conversations_of(&self, flow: FlowId) -> Vec<ConversationId> {
        self.conversations
            .values()
            .filter(|s| s.conversation.flow == flow)
            .map(|s| s.conversation.id)
            .collect()
    }

    pub(crate) fn notify(&mut self, flow: FlowId, title: String, body: String) {
        self.next_notification += 1;
        let list = Arc::make_mut(&mut self.notifications);
        list.push(Notification {
            id: self.next_notification,
            flow,
            title,
            body,
        });
        if list.len() > 20 {
            list.remove(0);
        }
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        let mut flows: Vec<FlowView> = self
            .flows
            .iter()
            .map(|c| {
                let conversations = self.conversations_of(c.id);
                let status = Status::of_flow(
                    conversations
                        .iter()
                        .filter_map(|id| self.conversations.get(id))
                        .map(|s| s.status),
                );
                FlowView {
                    flow: c.clone(),
                    status,
                    conversations,
                    issues: self.issues_of(c, &self.folders_of(c.id)),
                    folders: self.shown_folders(c.id),
                }
            })
            .collect();
        flows.sort_by_key(|c| std::cmp::Reverse(c.flow.updated_at));
        let conversations = self
            .conversations
            .values()
            .map(|s| {
                let view = ConversationView {
                    conversation: s.conversation.clone(),
                    status: s.status,
                    subagents: s.subagents,
                    plan: s.plan.clone(),
                    question: s.question.clone(),
                    suggestions: s.suggestions.clone(),
                    waiting_for: self.waiting_for(s.conversation.id),
                    name: self.conversation_name(s.conversation.id),
                    folder: self.folder_of(s),
                };
                (s.conversation.id, view)
            })
            .collect();
        let transcripts = self
            .conversations
            .values()
            .filter_map(|s| {
                s.transcript
                    .as_ref()
                    .map(|t| (s.conversation.id, Arc::new(t.clone())))
            })
            .collect();
        Snapshot {
            loaded: self.loaded,
            settings: self.settings.clone(),
            projects: Arc::clone(&self.projects),
            flows: Arc::new(flows),
            conversations: Arc::new(conversations),
            transcripts,
            agents: Arc::clone(&self.agents),
            commands: Arc::clone(&self.commands),
            folders: Arc::clone(&self.folders),
            folded_groups: Arc::clone(&self.folded_groups),
            todos: Arc::clone(&self.todos),
            options: Arc::clone(&self.options),
            notifications: Arc::clone(&self.notifications),
            git: Arc::clone(&self.git),
            prs: Arc::clone(&self.prs),
            linked_issues: Arc::clone(&self.linked_issues),
            linked_prs: Arc::clone(&self.linked_prs),
            my_issues: Arc::clone(&self.my_issues),
            my_prs: Arc::clone(&self.my_prs),
            github_error: self.github_error.clone(),
            detail: self.detail.clone(),
            pr_diff: self.pr_diff.clone(),
            fold: Arc::clone(&self.fold),
            flash: self.flash.clone(),
            doctor: self.doctor.clone(),
            doctor_running: self.doctor_running,
            shared_tools: Arc::clone(&self.shared),
            shared_tools_path: self.shared_path.clone(),
            settings_path: self.settings_path.clone(),
            shared_tools_error: self.shared_error.clone(),
            tools: self.tools.clone(),
            tools_pending: self.tools_pending.clone(),
            acting: Arc::clone(&self.acting),
            limits: Arc::clone(&self.limits),
            project_repos: Arc::clone(&self.project_repos),
        }
    }
}

/// A title from the first message (NAV-9): its first line, cut at a word
/// boundary around 60 characters.
pub(crate) fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("New flow")
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
        assert_eq!(title_from(""), "New flow");
        let long = "Implement the export of every invoice as CSV with the filters the user applied in the table";
        assert_eq!(
            title_from(long),
            "Implement the export of every invoice as CSV with the…"
        );
    }
}
