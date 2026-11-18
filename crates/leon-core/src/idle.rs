//! CLI processes of idle conversations are stopped after a while, then resumed by
//! their CLI session id on the next message (architecture.md, Processes).

use std::collections::HashMap;

use leon_domain::ConversationId;

use crate::app::Core;

/// How long a CLI process may wait for a message before it is stopped.
pub(crate) const IDLE_MS: i64 = 10 * 60 * 1000;

/// Conversations idle since longer than [`IDLE_MS`] at `now`.
fn expired(now: i64, idle_since: &HashMap<ConversationId, i64>) -> Vec<ConversationId> {
    let mut ids: Vec<_> = idle_since
        .iter()
        .filter(|(_, since)| now - **since >= IDLE_MS)
        .map(|(id, _)| *id)
        .collect();
    ids.sort();
    ids
}

impl Core {
    /// Stops the CLI processes that waited too long (on the core's tick).
    pub(crate) fn stop_idle_clis(&mut self) {
        let now = (self.ports.clock)();
        for id in expired(now, &self.idle_since) {
            self.idle_since.remove(&id);
            if self.live.remove(&id).is_some() {
                tracing::debug!(conversation = id.0, "stopped an idle CLI");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use leon_domain::ConversationId;

    use super::{IDLE_MS, expired};

    #[test]
    fn only_conversations_idle_long_enough_stop() {
        let since = HashMap::from([
            (ConversationId(1), 0),
            (ConversationId(2), 5),
            (ConversationId(3), IDLE_MS),
        ]);
        assert_eq!(
            expired(IDLE_MS + 5, &since),
            [ConversationId(1), ConversationId(2)]
        );
        assert_eq!(expired(IDLE_MS - 1, &since), Vec::new());
    }
}
