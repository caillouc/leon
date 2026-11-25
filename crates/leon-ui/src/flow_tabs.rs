//! A flow's agent tabs (FLOW-3): the conversations the user talks with and
//! those they opened, less those whose tab they closed.

use leon_domain::ConversationId;

/// The agent tabs the user opened in one flow, the last one (the one shown)
/// first. Closed tabs are kept with their conversation, across restarts.
#[derive(Debug, Default)]
pub(crate) struct FlowTabs {
    opened: Vec<ConversationId>,
}

impl FlowTabs {
    /// `conversation` is shown: its tab comes first after Flow.
    pub(crate) fn open(&mut self, conversation: ConversationId) {
        self.opened.retain(|c| *c != conversation);
        self.opened.insert(0, conversation);
    }

    /// The user closed `conversation`'s tab; it stays in the flow.
    pub(crate) fn close(&mut self, conversation: ConversationId) {
        self.opened.retain(|c| *c != conversation);
    }

    /// The agent tabs after Flow: those opened, the last one first, then
    /// the others with a tab of their own (`listed`: the user talks with
    /// them and did not close it) in start order.
    pub(crate) fn order(
        &self,
        all: &[ConversationId],
        listed: impl Fn(ConversationId) -> bool,
    ) -> Vec<ConversationId> {
        let mut order: Vec<ConversationId> = self
            .opened
            .iter()
            .filter(|c| all.contains(c))
            .copied()
            .collect();
        order.extend(
            all.iter()
                .copied()
                .filter(|c| !self.opened.contains(c) && listed(*c)),
        );
        order
    }
}

#[cfg(test)]
mod tests {
    use leon_domain::ConversationId;

    use super::FlowTabs;

    #[test]
    fn tabs_are_those_opened_last_first_then_the_others_listed() {
        let ids = |list: &[i64]| list.iter().map(|i| ConversationId(*i)).collect::<Vec<_>>();
        let all = ids(&[1, 2, 3, 4, 5]);
        // The user talks with 1 and 3, and did not close their tabs; agents
        // alone talk with 2, 4 and 5.
        let listed = |c: ConversationId| c.0 == 1 || c.0 == 3;
        // Positive: opened; negative: closed.
        let cases: [(&[i64], &[i64]); 5] = [
            (&[], &[1, 3]),
            (&[4], &[4, 1, 3]),
            (&[4, 3], &[3, 4, 1]),
            (&[4, -4], &[1, 3]),
            // One no longer in the flow is skipped.
            (&[3, 9], &[3, 1]),
        ];
        for (steps, expected) in cases {
            let mut tabs = FlowTabs::default();
            for step in steps {
                let c = ConversationId(step.abs());
                if *step > 0 {
                    tabs.open(c);
                } else {
                    tabs.close(c);
                }
            }
            assert_eq!(tabs.order(&all, listed), ids(expected), "{steps:?}");
        }
    }
}
