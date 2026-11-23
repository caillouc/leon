//! Coming back to the CLIs that kept working while Leon was closed (I24):
//! each one still running is live again; one that was in a turn shows
//! working, its turn replayed from its start, so the UI matches what the
//! agent did meanwhile.

use std::sync::Arc;

use leon_domain::{ConversationId, Entry, Status};

use crate::app::Core;

impl Core {
    /// Finds the CLIs still running from before Leon started; what is left
    /// of a conversation deleted since goes (NAV-8).
    pub(crate) fn reattach(&mut self) {
        let backends = Arc::clone(&self.ports.backends);
        let mut gone = Vec::new();
        for id in backends.held() {
            if !self.state.conversations.contains_key(&id) {
                gone.push((id, None));
                continue;
            }
            let Some(spec) = self.session_spec(id) else {
                continue;
            };
            let Some(attached) = backends.attach(spec) else {
                continue;
            };
            if attached.busy {
                self.replay_from_turn_start(id);
            }
            self.pump(id, attached.channel);
        }
        self.discard(gone);
    }

    /// Conversation `id` was in a turn: what Leon recorded of it goes, the
    /// replayed turn brings it back whole, and it shows working.
    fn replay_from_turn_start(&mut self, id: ConversationId) {
        self.load_transcript(id);
        let Some(s) = self.state.conversations.get_mut(&id) else {
            return;
        };
        s.status = Status::Running;
        let Some(transcript) = s.transcript.as_mut() else {
            return;
        };
        let kept = turn_start(
            &transcript
                .entries
                .iter()
                .map(|e| &e.entry)
                .collect::<Vec<_>>(),
        );
        if kept == transcript.entries.len() {
            return;
        }
        let entries = Arc::make_mut(&mut transcript.entries);
        entries.truncate(kept);
        let kept = entries.clone();
        let storage = Arc::clone(&self.ports.storage);
        Self::report(crate::app::blocking(|| storage.replace_entries(id, &kept)));
    }
}

/// How many entries to keep: up to the last message sent to the agent
/// (the user's, another agent's, or a request for the user), which starts
/// the turn being replayed.
pub(crate) fn turn_start(entries: &[&Entry]) -> usize {
    entries
        .iter()
        .rposition(|e| {
            matches!(
                e,
                Entry::User { .. } | Entry::FromAgent { .. } | Entry::ForYou { .. }
            )
        })
        .map_or(0, |i| i + 1)
}
