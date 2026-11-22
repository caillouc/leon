//! How a CLI event changes an conversation.

use leon_domain::{ConversationId, Entry, Status};

use crate::app::Core;
use crate::ports::BackendEvent;
use crate::snapshot::Question;

pub(crate) fn apply(core: &mut Core, id: ConversationId, event: BackendEvent) {
    match event {
        BackendEvent::Ready { cli_session } => {
            if let Some(s) = core.state_mut().conversations.get_mut(&id) {
                s.conversation.cli_session = Some(cli_session);
            }
            core.save_conversation(id);
        }
        BackendEvent::TextDelta(delta) => {
            if let Some(t) = core
                .state_mut()
                .conversations
                .get_mut(&id)
                .and_then(|s| s.transcript.as_mut())
            {
                t.partial.get_or_insert_with(String::new).push_str(&delta);
            }
        }
        BackendEvent::Text(text) => {
            if let Some(t) = core
                .state_mut()
                .conversations
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
        } => tool_started(core, id, tool, name, detail),
        BackendEvent::FilesChanged(paths) => core.files_changed(id, &paths),
        BackendEvent::ToolDone {
            id: tool,
            output,
            failed,
        } => tool_done(core, id, &tool, output, failed),
        BackendEvent::Notice(text) => {
            core.record_entry(id, Entry::Notice { text });
        }
        BackendEvent::Suggestion(text) => {
            if let Some(s) = core.state_mut().conversations.get_mut(&id)
                && !s.suggestions.contains(&text)
            {
                s.suggestions.push(text);
            }
        }
        BackendEvent::Plan(steps) => {
            if let Some(s) = core.state_mut().conversations.get_mut(&id) {
                s.plan = steps;
            }
        }
        BackendEvent::Subagents { started, running } => {
            if let Some(s) = core.state_mut().conversations.get_mut(&id) {
                s.subagents = (started, running);
            }
        }
        BackendEvent::Question {
            id: question,
            title,
            detail,
            options,
        } => asked(
            core,
            id,
            Question {
                id: question,
                title,
                detail,
                options,
            },
        ),
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
            if let Some(s) = core.state_mut().conversations.get_mut(&id) {
                s.status = Status::Error;
            }
            core.record_entry(id, Entry::Error { text });
        }
        BackendEvent::Limits(windows) => limits(core, id, windows),
    }
}

fn tool_started(core: &mut Core, id: ConversationId, tool: String, name: String, detail: String) {
    let entry = Entry::Tool {
        id: tool.clone(),
        name,
        detail,
        output: None,
        failed: false,
    };
    if let Some(index) = core.record_entry(id, entry)
        && let Some(s) = core.state_mut().conversations.get_mut(&id)
    {
        s.pending_tools.insert(tool, index);
    }
}

/// The CLI waits for the user (CONV-9).
fn asked(core: &mut Core, id: ConversationId, question: Question) {
    let Some(s) = core.state_mut().conversations.get_mut(&id) else {
        return;
    };
    s.status = Status::NeedsInput;
    s.question = Some(question);
    let flow = s.conversation.flow;
    core.answered(id, flow, false);
}

fn tool_done(core: &mut Core, id: ConversationId, tool: &str, output: String, failed: bool) {
    let Some(s) = core.state_mut().conversations.get_mut(&id) else {
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
    let flow = s.conversation.flow;
    core.save_entry(id, &done);
    if let leon_domain::Entry::Tool { name, .. } = &done.entry
        && matches!(name.as_str(), "Edited" | "Wrote" | "Ran")
    {
        core.refresh_git(flow);
    }
}

/// Keeps what conversation `id`'s CLI says of the plan's limits, for that
/// CLI (DOCTOR-2).
fn limits(core: &mut Core, id: ConversationId, windows: Vec<leon_domain::LimitWindow>) {
    let state = core.state_mut();
    let Some(backend) = state
        .conversations
        .get(&id)
        .map(|s| s.conversation.settings.backend)
    else {
        return;
    };
    let all = std::sync::Arc::make_mut(&mut state.limits);
    all.retain(|(b, _)| *b != backend);
    all.push((backend, windows));
    all.sort_by_key(|(b, _)| leon_domain::BackendKind::ALL.iter().position(|x| x == b));
}
