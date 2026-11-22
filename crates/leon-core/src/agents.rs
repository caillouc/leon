//! Agents working together (FLOW-1, FLOW-2, P15): tool calls from the app's
//! MCP server, messages between conversations, answers sent back to the
//! conversation that asked.

use std::path::{Path, PathBuf};

use leon_domain::{ConversationId, Entry, FlowId, GhRef, Status};

use crate::app::Core;
use crate::ports::{AppToolCall, AppToolRequest, NewConversation};
use crate::state::{ConversationState, Sent, Waiting};

impl Core {
    pub(crate) fn on_tool(&mut self, request: AppToolRequest) {
        let AppToolRequest {
            conversation,
            call,
            reply,
        } = request;
        let result = self.tool(conversation, call);
        if reply.send(result).is_err() {
            tracing::debug!(
                conversation = conversation.0,
                "the agent left before its tool call ended"
            );
        }
    }

    fn tool(&mut self, caller: ConversationId, call: AppToolCall) -> Result<String, String> {
        let flow = self
            .state
            .conversations
            .get(&caller)
            .map(|s| s.conversation.flow)
            .ok_or_else(|| "Leon does not know this conversation.".to_owned())?;
        match call {
            AppToolCall::StartAgent {
                agent,
                prompt,
                working_directory,
                for_user,
            } => {
                let sent = if for_user {
                    Sent::ForUser
                } else {
                    Sent::Request
                };
                self.start_agent(caller, flow, &agent, &prompt, working_directory, sent)
            }
            AppToolCall::MessageAgent { conversation, text } => {
                let target = self
                    .state
                    .find_conversation(flow, &conversation)
                    .ok_or_else(|| {
                        format!(
                            "No conversation of this flow is called \"{conversation}\". Its \
                         conversations: {}.",
                            self.state.roll_call(flow)
                        )
                    })?;
                self.message_agent(caller, flow, target, &text)
            }
            AppToolCall::RenameFlow { title } => {
                let title = title.trim().to_owned();
                if title.is_empty() {
                    return Err("The title is empty.".to_owned());
                }
                self.edit(flow, |c| c.title = title);
                Ok("Renamed the flow.".to_owned())
            }
            AppToolCall::SetWorkingDirectory { path } => {
                check_dir(&path)?;
                let shown = path.display().to_string();
                self.find_working_tree(caller, flow, path, true);
                Ok(format!("Leon now shows the changes in {shown}."))
            }
            AppToolCall::LinkGithub { url } => {
                let item = GhRef::from_url(url.trim()).ok_or_else(|| {
                    format!(
                        "{url} is not the link of a GitHub issue or pull request, like \
                         https://github.com/owner/name/issues/12 or …/pull/12."
                    )
                })?;
                let text = item.text();
                self.link(flow, item);
                Ok(format!("Linked {text} to the flow: Leon shows it in Info."))
            }
            AppToolCall::AddTodo { when, what } => {
                // An agent's to-do waits for something: a step the user can
                // take now belongs in its answer (TODO-1).
                if when.trim().is_empty() {
                    return Err("`when` is empty: a to-do waits for something (a deploy, a \
                        date, someone's answer). A step the user can take now goes in your \
                        answer, not in a to-do."
                        .to_owned());
                }
                self.add_todo(flow, (&when, &what), Some(caller))
            }
            AppToolCall::ListTodos => Ok(self.list_todos(flow)),
        }
    }

    fn start_agent(
        &mut self,
        caller: ConversationId,
        flow: FlowId,
        agent: &str,
        prompt: &str,
        working_directory: Option<PathBuf>,
        sent: Sent,
    ) -> Result<String, String> {
        if prompt.trim().is_empty() {
            return Err("The prompt is empty.".to_owned());
        }
        let project = self
            .state
            .flows
            .iter()
            .find(|c| c.id == flow)
            .map(|c| c.project)
            .ok_or_else(|| "This flow no longer exists.".to_owned())?;
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
        let given = working_directory.is_some();
        let working_dir = match working_directory {
            Some(path) => {
                check_dir(&path)?;
                path
            }
            None => self
                .state
                .conversations
                .get(&caller)
                .map(|s| s.conversation.working_dir.clone())
                .ok_or_else(|| "Leon does not know this conversation.".to_owned())?,
        };
        let mut settings = self
            .state
            .conversations
            .get(&caller)
            .map(|s| s.conversation.settings.clone())
            .ok_or_else(|| "Leon does not know this conversation.".to_owned())?;
        settings.agent = Some(agent.to_owned());
        // Its file's model picks its CLI (EXT-6); else the caller's (I7).
        let model = self
            .state
            .agents
            .get(&project)
            .and_then(|list| list.iter().find(|a| a.name == agent))
            .and_then(|a| a.model.clone());
        if let Some(next) = crate::settings_for_agent(
            &settings,
            model.as_deref(),
            &self.state.settings.cli_order,
            &self.state.options,
        ) {
            settings = next;
        }
        let new = NewConversation {
            flow,
            started_by: Some(caller),
            settings,
            working_dir,
        };
        let id = self.add_conversation(&new).map_err(|e| e.to_string())?;
        // Started in a folder of its own: the folder joins the flow at once.
        if given {
            self.find_working_tree(id, flow, new.working_dir.clone(), true);
        }
        self.message(caller, id, prompt.to_owned(), sent);
        let name = self.state.conversation_name(id);
        if sent == Sent::ForUser {
            return Ok(format!(
                "Started {name} (conversation {n}) for the user: its answers go to the user, \
                 who reviews and continues it; nothing comes back here. The flow's \
                 conversations: {all}.",
                n = id.0,
                all = self.state.roll_call(flow)
            ));
        }
        Ok(format!(
            "Started {name} (conversation {n}). Its answer will arrive here as a new message \
             starting with [From {name} · conversation {n}]. For follow-ups, use message_agent \
             with conversation \"{name}\" or {n}. The flow's conversations: {all}.",
            n = id.0,
            all = self.state.roll_call(flow)
        ))
    }

    fn message_agent(
        &mut self,
        caller: ConversationId,
        flow: FlowId,
        target: ConversationId,
        text: &str,
    ) -> Result<String, String> {
        if target == caller {
            return Err("A conversation cannot message itself.".to_owned());
        }
        if text.trim().is_empty() {
            return Err("The message is empty.".to_owned());
        }
        let busy = self
            .state
            .conversations
            .get(&target)
            .filter(|s| s.conversation.flow == flow)
            .map(|s| !ready(s.status))
            .ok_or_else(|| format!("Conversation {} is not in this flow.", target.0))?;
        let name = self.state.conversation_name(target);
        self.message(caller, target, text.to_owned(), Sent::Request);
        Ok(if busy {
            format!(
                "{name} (conversation {}) is busy: your message is sent when it finishes. \
                 Its answer will arrive here as a new message.",
                target.0
            )
        } else {
            format!(
                "Sent to {name} (conversation {}). Its answer will arrive here as a new message.",
                target.0
            )
        })
    }

    /// Sends `text` from conversation `from` to conversation `to`: now if `to` is ready,
    /// else after its current turn. For a request, `to`'s answer goes back to
    /// `from`.
    fn message(&mut self, from: ConversationId, to: ConversationId, text: String, sent: Sent) {
        let name = self.state.conversation_name(from);
        let header = match sent {
            Sent::ForUser => format!(
                "[From {name} · conversation {} · for the user: your answers go to the user, \
                 who reviews and continues this conversation, not back to {name}]",
                from.0
            ),
            Sent::Request | Sent::Answer => format!("[From {name} · conversation {}]", from.0),
        };
        let Some(s) = self.state.conversations.get_mut(&to) else {
            return;
        };
        if !ready(s.status) {
            s.inbox.push_back(Waiting { from, text, sent });
            return;
        }
        if sent == Sent::Request {
            s.reply_to = Some(from);
        }
        let message = format!("{header}\n\n{text}");
        let entry = match sent {
            Sent::Request | Sent::Answer => Entry::FromAgent {
                from,
                text,
                answer: sent == Sent::Answer,
            },
            Sent::ForUser => Entry::ForYou { from, text },
        };
        if !self.deliver(to, entry, &message) {
            // The CLI could not start: the asker gets the error as the answer.
            self.after_turn(to, true);
        }
    }

    /// After a turn: the answer goes back to the conversation that asked —
    /// also for a turn its CLI started by itself — then the next waiting
    /// message is sent. `true` when the answer went to another
    /// conversation rather than to the user.
    pub(crate) fn after_turn(&mut self, id: ConversationId, failed: bool) -> bool {
        let Some(s) = self.state.conversations.get_mut(&id) else {
            return false;
        };
        // The asker waiting for this turn, else the one whose request it
        // works on: a turn its CLI started by itself answers there too.
        let reply_to = s.reply_to.take().or_else(|| works_for(s));
        let answer = reply_to.map(|_| last_answer(s, failed));
        let next = s.inbox.pop_front();
        if let (Some(to), Some(answer)) = (reply_to, answer) {
            self.message(id, to, answer, Sent::Answer);
        }
        if let Some(w) = next {
            self.message(w.from, id, w.text, w.sent);
        }
        reply_to.is_some()
    }

    /// Conversation `id`'s work goes on: it already reads its next message,
    /// or an agent it asked has not answered yet.
    pub(crate) fn goes_on(&self, id: ConversationId) -> bool {
        self.state
            .conversations
            .get(&id)
            .is_some_and(|s| s.status == Status::Running)
            || !self.state.waiting_for(id).is_empty()
    }
}

/// The conversation whose request `s` works on, by its transcript
/// ([`leon_domain::answers_to`]); `None` when it works for the user.
fn works_for(s: &ConversationState) -> Option<ConversationId> {
    s.transcript.as_ref().map_or_default(|t| {
        t.entries
            .iter()
            .fold(None, |to, e| leon_domain::answers_to(to, &e.entry))
    })
}

/// A conversation can take a message now: it is not in a turn.
fn ready(status: Status) -> bool {
    matches!(status, Status::Idle | Status::Error)
}

/// The answer of the turn that just ended: its agent messages since the
/// last message it received or its previous turn's end (a turn its CLI
/// started by itself), plus its error when it failed.
fn last_answer(s: &ConversationState, failed: bool) -> String {
    let entries = s
        .transcript
        .as_ref()
        .map_or_default(|t| t.entries.as_slice());
    // This turn's own end is already recorded: the turn before it ends
    // further up.
    let entries = match entries.split_last() {
        Some((last, before)) if matches!(last.entry, Entry::TurnEnd { .. }) => before,
        _ => entries,
    };
    let start = entries
        .iter()
        .rposition(|e| {
            matches!(
                e.entry,
                Entry::User { .. }
                    | Entry::FromAgent { .. }
                    | Entry::ForYou { .. }
                    | Entry::TurnEnd { .. }
            )
        })
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
