//! How a CLI event changes an agent session.

use leon_domain::{AgentSessionId, Entry, Status};

use crate::app::Core;
use crate::ports::BackendEvent;
use crate::snapshot::Question;

pub(crate) fn apply(core: &mut Core, id: AgentSessionId, event: BackendEvent) {
    match event {
        BackendEvent::Ready { cli_session } => {
            if let Some(s) = core.state_mut().sessions.get_mut(&id) {
                s.session.cli_session = Some(cli_session);
            }
            core.save_session(id);
        }
        BackendEvent::TextDelta(delta) => {
            if let Some(t) = core
                .state_mut()
                .sessions
                .get_mut(&id)
                .and_then(|s| s.transcript.as_mut())
            {
                t.partial.get_or_insert_with(String::new).push_str(&delta);
            }
        }
        BackendEvent::Text(text) => {
            if let Some(t) = core
                .state_mut()
                .sessions
                .get_mut(&id)
                .and_then(|s| s.transcript.as_mut())
            {
                t.partial = None;
            }
            core.record_entry(id, Entry::Agent { text });
        }
        BackendEvent::Tool {
            id: tool,
            name,
            detail,
        } => {
            let entry = Entry::Tool {
                id: tool.clone(),
                name,
                detail,
                output: None,
                failed: false,
            };
            if let Some(index) = core.record_entry(id, entry)
                && let Some(s) = core.state_mut().sessions.get_mut(&id)
            {
                s.pending_tools.insert(tool, index);
            }
        }
        BackendEvent::ToolDone {
            id: tool,
            output,
            failed,
        } => tool_done(core, id, &tool, output, failed),
        BackendEvent::Subagents { started, running } => {
            if let Some(s) = core.state_mut().sessions.get_mut(&id) {
                s.subagents = (started, running);
            }
        }
        BackendEvent::Question {
            id: question,
            title,
            detail,
            options,
        } => {
            let Some(s) = core.state_mut().sessions.get_mut(&id) else {
                return;
            };
            s.status = Status::NeedsInput;
            s.question = Some(Question {
                id: question,
                title,
                detail,
                options,
            });
            let conversation = s.session.conversation;
            core.answered(id, conversation, false);
        }
        BackendEvent::TurnEnded { stopped, usage } => {
            if stopped {
                core.record_entry(
                    id,
                    Entry::Notice {
                        text: "Stopped by you".to_owned(),
                    },
                );
            }
            core.finish_turn(id, usage, None);
        }
        BackendEvent::Failed(text) => {
            if let Some(s) = core.state_mut().sessions.get_mut(&id) {
                s.status = Status::Error;
            }
            core.record_entry(id, Entry::Error { text });
        }
    }
}

fn tool_done(core: &mut Core, id: AgentSessionId, tool: &str, output: String, failed: bool) {
    let Some(s) = core.state_mut().sessions.get_mut(&id) else {
        return;
    };
    let Some(index) = s.pending_tools.remove(tool) else {
        return;
    };
    let Some(t) = s.transcript.as_mut() else {
        return;
    };
    let entries = std::sync::Arc::make_mut(&mut t.entries);
    let Some(timed) = entries.get_mut(index) else {
        return;
    };
    if let Entry::Tool {
        output: o,
        failed: f,
        ..
    } = &mut timed.entry
    {
        *o = Some(output);
        *f = failed;
    }
    let done = timed.clone();
    let conversation = s.session.conversation;
    core.save_entry(id, &done);
    if let leon_domain::Entry::Tool { name, .. } = &done.entry
        && matches!(name.as_str(), "Edited" | "Wrote" | "Ran")
    {
        core.refresh_git(conversation);
    }
}
