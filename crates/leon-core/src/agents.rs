//! Agents working together (FLOW-1, FLOW-2, P15): tool calls from the app's
//! MCP server, messages between agent sessions, answers sent back to the
//! session that asked.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use leon_domain::{AgentSessionId, ConversationId, Entry, Status};

use crate::app::Core;
use crate::ports::{AppToolCall, AppToolRequest, NewSession};
use crate::snapshot::TranscriptView;
use crate::state::{SessionState, Waiting};

impl Core {
    pub(crate) fn on_tool(&mut self, request: AppToolRequest) {
        let AppToolRequest {
            session,
            call,
            reply,
        } = request;
        let result = self.tool(session, call);
        if reply.send(result).is_err() {
            tracing::debug!(
                session = session.0,
                "the agent left before its tool call ended"
            );
        }
    }

    fn tool(&mut self, caller: AgentSessionId, call: AppToolCall) -> Result<String, String> {
        let conversation = self
            .state
            .sessions
            .get(&caller)
            .map(|s| s.session.conversation)
            .ok_or_else(|| "Leon does not know this agent session.".to_owned())?;
        match call {
            AppToolCall::StartAgent {
                agent,
                prompt,
                working_directory,
            } => self.start_agent(caller, conversation, &agent, &prompt, working_directory),
            AppToolCall::MessageAgent { session, text } => {
                self.message_agent(caller, conversation, session, &text)
            }
            AppToolCall::RenameConversation { title } => {
                let title = title.trim().to_owned();
                if title.is_empty() {
                    return Err("The title is empty.".to_owned());
                }
                self.edit(conversation, |c| c.title = title);
                Ok("Renamed the conversation.".to_owned())
            }
            AppToolCall::SetWorkingDirectory { path } => {
                check_dir(&path)?;
                let shown = path.display().to_string();
                self.edit(conversation, |c| c.working_dir = Some(path));
                self.refresh_git(conversation);
                Ok(format!("Leon now shows the changes in {shown}."))
            }
        }
    }

    fn start_agent(
        &mut self,
        caller: AgentSessionId,
        conversation: ConversationId,
        agent: &str,
        prompt: &str,
        working_directory: Option<PathBuf>,
    ) -> Result<String, String> {
        if prompt.trim().is_empty() {
            return Err("The prompt is empty.".to_owned());
        }
        let project = self
            .state
            .conversations
            .iter()
            .find(|c| c.id == conversation)
            .map(|c| c.project)
            .ok_or_else(|| "This conversation no longer exists.".to_owned())?;
        let names: Vec<String> = self
            .state
            .agents
            .get(&project)
            .map_or_default(|list| list.iter().map(|a| a.name.clone()).collect());
        if !names.iter().any(|n| n == agent) {
            return Err(if names.is_empty() {
                format!("No agent named \"{agent}\": this project has no agent files.")
            } else {
                format!("No agent named \"{agent}\". Agents: {}.", names.join(", "))
            });
        }
        let working_dir = match working_directory {
            Some(path) => {
                check_dir(&path)?;
                path
            }
            None => self
                .conversation_dir(conversation)
                .ok_or_else(|| "This conversation has no folder.".to_owned())?,
        };
        let mut settings = self
            .state
            .sessions
            .get(&caller)
            .map(|s| s.session.settings.clone())
            .ok_or_else(|| "Leon does not know this agent session.".to_owned())?;
        settings.agent = Some(agent.to_owned());
        let new = NewSession {
            conversation,
            started_by: Some(caller),
            settings,
            working_dir,
        };
        let storage = Arc::clone(&self.ports.storage);
        let session = tokio::task::block_in_place(|| storage.create_session(&new))
            .map_err(|e| e.to_string())?;
        let id = session.id;
        let mut state = SessionState::new(session);
        state.transcript = Some(TranscriptView::default());
        self.state.sessions.insert(id, state);
        self.message(caller, id, prompt.to_owned(), true);
        Ok(format!(
            "Started {agent} as session {n}. Its answer will arrive here as a new message \
             starting with [From {agent} · session {n}]. Use message_agent with session {n} \
             for follow-ups.",
            n = id.0
        ))
    }

    fn message_agent(
        &mut self,
        caller: AgentSessionId,
        conversation: ConversationId,
        target: AgentSessionId,
        text: &str,
    ) -> Result<String, String> {
        if target == caller {
            return Err("A session cannot message itself.".to_owned());
        }
        if text.trim().is_empty() {
            return Err("The message is empty.".to_owned());
        }
        let busy = self
            .state
            .sessions
            .get(&target)
            .filter(|s| s.session.conversation == conversation)
            .map(|s| !ready(s.status))
            .ok_or_else(|| format!("Session {} is not in this conversation.", target.0))?;
        let name = self.session_label(target);
        self.message(caller, target, text.to_owned(), true);
        Ok(if busy {
            format!(
                "{name} (session {}) is busy: your message is sent when it finishes. \
                 Its answer will arrive here as a new message.",
                target.0
            )
        } else {
            format!(
                "Sent to {name} (session {}). Its answer will arrive here as a new message.",
                target.0
            )
        })
    }

    /// Sends `text` from session `from` to session `to`: now if `to` is ready,
    /// else after its current turn. For a request, `to`'s answer goes back to
    /// `from`.
    fn message(
        &mut self,
        from: AgentSessionId,
        to: AgentSessionId,
        text: String,
        expects_answer: bool,
    ) {
        let header = format!("[From {} · session {}]", self.session_label(from), from.0);
        let Some(s) = self.state.sessions.get_mut(&to) else {
            return;
        };
        if !ready(s.status) {
            s.inbox.push_back(Waiting {
                from,
                text,
                expects_answer,
            });
            return;
        }
        if expects_answer {
            s.reply_to = Some(from);
        }
        let message = format!("{header}\n\n{text}");
        let entry = Entry::FromAgent {
            from,
            text,
            answer: !expects_answer,
        };
        if !self.deliver(to, entry, &message) {
            // The CLI could not start: the asker gets the error as the answer.
            self.after_turn(to, true);
        }
    }

    /// After a turn: the answer goes back to the session that asked, then the
    /// next waiting message is sent. `true` when the answer went to another
    /// session rather than to the user.
    pub(crate) fn after_turn(&mut self, id: AgentSessionId, failed: bool) -> bool {
        let Some(s) = self.state.sessions.get_mut(&id) else {
            return false;
        };
        let reply_to = s.reply_to.take();
        let answer = reply_to.map(|_| last_answer(s, failed));
        let next = s.inbox.pop_front();
        if let (Some(to), Some(answer)) = (reply_to, answer) {
            self.message(id, to, answer, false);
        }
        if let Some(w) = next {
            self.message(w.from, id, w.text, w.expects_answer);
        }
        reply_to.is_some()
    }

    /// The agent's name, or its CLI's when it has no agent.
    pub(crate) fn session_label(&self, id: AgentSessionId) -> String {
        self.state.sessions.get(&id).map_or_else(
            || format!("session {}", id.0),
            |s| {
                s.session
                    .settings
                    .agent
                    .clone()
                    .unwrap_or_else(|| s.session.settings.backend.display_name().to_owned())
            },
        )
    }

    /// The folder a conversation works in (D3): where an agent said it moved,
    /// else the folder of the session the user started.
    pub(crate) fn conversation_dir(&self, id: ConversationId) -> Option<PathBuf> {
        self.state
            .conversations
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.working_dir.clone())
            .or_else(|| {
                self.state
                    .sessions
                    .values()
                    .filter(|s| s.session.conversation == id)
                    .find(|s| s.session.started_by.is_none())
                    .map(|s| s.session.working_dir.clone())
            })
    }
}

/// A session can take a message now: it is not in a turn.
fn ready(status: Status) -> bool {
    matches!(status, Status::Idle | Status::Error)
}

/// The answer of the turn that just ended: its agent messages after the last
/// message it received, plus its error when it failed.
fn last_answer(s: &SessionState, failed: bool) -> String {
    let entries = s
        .transcript
        .as_ref()
        .map_or_default(|t| t.entries.as_slice());
    let start = entries
        .iter()
        .rposition(|e| matches!(e.entry, Entry::User { .. } | Entry::FromAgent { .. }))
        .map_or(0, |i| i + 1);
    let mut parts: Vec<&str> = Vec::new();
    for e in &entries[start..] {
        match &e.entry {
            Entry::Agent { text } => parts.push(text),
            Entry::Error { text } if failed => parts.push(text),
            _ => {}
        }
    }
    match (parts.is_empty(), failed) {
        (true, true) => "It stopped with an error before answering.".to_owned(),
        (true, false) => "It finished without a written answer.".to_owned(),
        (false, true) => format!("It stopped with an error.\n\n{}", parts.join("\n\n")),
        (false, false) => parts.join("\n\n"),
    }
}

/// An absolute path to an existing folder.
fn check_dir(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("{} is not an absolute path.", path.display()));
    }
    if !tokio::task::block_in_place(|| path.is_dir()) {
        return Err(format!("{} is not a folder.", path.display()));
    }
    Ok(())
}
