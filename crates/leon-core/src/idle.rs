//! CLI processes of idle sessions are stopped after a while, then resumed by
//! their CLI session id on the next message (architecture.md, Processes).

use std::collections::HashMap;

use leon_domain::AgentSessionId;

use crate::app::Core;

/// How long a CLI process may wait for a message before it is stopped.
pub(crate) const IDLE_MS: i64 = 10 * 60 * 1000;

/// Sessions idle since longer than [`IDLE_MS`] at `now`.
fn expired(now: i64, idle_since: &HashMap<AgentSessionId, i64>) -> Vec<AgentSessionId> {
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
                tracing::debug!(session = id.0, "stopped an idle CLI");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use leon_domain::AgentSessionId;

    use super::{IDLE_MS, expired};

    #[test]
    fn only_sessions_idle_long_enough_stop() {
        let since = HashMap::from([
            (AgentSessionId(1), 0),
            (AgentSessionId(2), 5),
            (AgentSessionId(3), IDLE_MS),
        ]);
        assert_eq!(
            expired(IDLE_MS + 5, &since),
            [AgentSessionId(1), AgentSessionId(2)]
        );
        assert_eq!(expired(IDLE_MS - 1, &since), Vec::new());
    }
}
