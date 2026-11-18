//! Each agent's colour, the same in the flow diagram, the right pane and
//! its tabs (FLOW-4, CTX-1).

use gpui::Rgba;
use leon_core::ConversationView;

use crate::theme;

/// The colour of `conversation`'s agent among a flow's `conversations`: one
/// per agent (its CLI's own agent counts as one), in the order the agents
/// first appear, so two agents never share one and every conversation of
/// one agent has its colour.
pub(crate) fn agent_color(
    conversations: &[&ConversationView],
    conversation: &ConversationView,
) -> Rgba {
    let mut agents: Vec<String> = Vec::new();
    for c in conversations {
        let agent = agent_of(c);
        if !agents.contains(&agent) {
            agents.push(agent);
        }
    }
    let ix = agents
        .iter()
        .position(|a| *a == agent_of(conversation))
        .unwrap_or(0);
    theme::agent_color(ix)
}

/// The agent a conversation talks to: its agent file's name, else its
/// CLI's (the flow diagram's regions follow the same rule).
fn agent_of(c: &ConversationView) -> String {
    let settings = &c.conversation.settings;
    settings
        .agent
        .clone()
        .unwrap_or_else(|| settings.backend.display_name().to_owned())
}
