//! Back and forward through the places the user opened (NAV-12): a flow on
//! its Flow tab or on one conversation's tab, or a page (To do, a GitHub
//! list…), in the order visited, like a browser's history.

use leon_core::Snapshot;
use leon_domain::{ConversationId, FlowId};

use crate::workspace::Screen;

/// A place back and forward go through (NAV-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// A flow, on its Flow tab or on a conversation's.
    Flow(FlowId, Option<ConversationId>),
    /// A page of its own: To do, a GitHub list, Doctor, Connectors, Settings.
    Page(Screen),
}

impl Place {
    /// Where the main pane is, showing `screen` (and `reading`'s tab on a
    /// flow); `None` on what is no place to come back to: nothing, a new
    /// flow's form.
    pub(crate) fn of(screen: Screen, reading: Option<ConversationId>) -> Option<Self> {
        match screen {
            Screen::Flow(id) => Some(Self::Flow(id, reading)),
            Screen::GitHub(_)
            | Screen::Doctor
            | Screen::Tools
            | Screen::Settings
            | Screen::Todos => Some(Self::Page(screen)),
            Screen::Nothing | Screen::NewFlow => None,
        }
    }

    /// Whether the place is still there: a flow not deleted, a tab whose
    /// conversation is still in it; a page always is.
    pub(crate) fn exists(self, snapshot: &Snapshot) -> bool {
        match self {
            Self::Flow(flow, tab) => snapshot
                .flow(flow)
                .is_some_and(|f| tab.is_none_or(|c| f.conversations.contains(&c))),
            Self::Page(_) => true,
        }
    }
}

/// How many places are kept.
const KEPT: usize = 50;

/// The places visited, and where the user is among them.
#[derive(Debug)]
pub(crate) struct History<T> {
    places: Vec<T>,
    at: usize,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            places: Vec::new(),
            at: 0,
        }
    }
}

impl<T: PartialEq + Clone> History<T> {
    /// The user is now at `place`: after going back, the places ahead go.
    pub(crate) fn visit(&mut self, place: T) {
        if self.places.get(self.at) == Some(&place) {
            return;
        }
        if !self.places.is_empty() {
            self.places.truncate(self.at + 1);
        }
        self.places.push(place);
        if self.places.len() > KEPT {
            self.places.remove(0);
        }
        self.at = self.places.len() - 1;
    }

    /// The place before, skipping those `there` says are gone.
    pub(crate) fn back(&mut self, there: impl Fn(&T) -> bool) -> Option<T> {
        let at = (0..self.at).rev().find(|&i| there(&self.places[i]))?;
        self.at = at;
        Some(self.places[at].clone())
    }

    /// The place after, skipping those `there` says are gone.
    pub(crate) fn forward(&mut self, there: impl Fn(&T) -> bool) -> Option<T> {
        let at = (self.at + 1..self.places.len()).find(|&i| there(&self.places[i]))?;
        self.at = at;
        Some(self.places[at].clone())
    }
}

#[cfg(test)]
mod tests {
    use leon_core::{FlowView, Snapshot};
    use leon_domain::{ConversationId, Flow, FlowId, ProjectId, Status};

    use super::{History, Place};
    use crate::workspace::Screen;

    #[test]
    fn a_page_is_a_place_and_gone_places_are_skipped() {
        let flow = FlowView {
            flow: Flow {
                id: FlowId(1),
                project: ProjectId(1),
                title: "Flow".into(),
                pinned: false,
                archived: None,
                unread: false,
                updated_at: 0,
                folders: Vec::new(),
                links: Vec::new(),
                todo_position: 0,
            },
            status: Status::Idle,
            conversations: vec![ConversationId(10)],
            folders: Vec::new(),
            issues: Vec::new(),
        };
        let snapshot = Snapshot {
            flows: vec![flow].into(),
            ..Snapshot::default()
        };
        let there = |place: &Place| place.exists(&snapshot);
        let on_flow = Place::Flow(FlowId(1), None);
        let on_tab = Place::Flow(FlowId(1), Some(ConversationId(10)));
        let todos = Place::of(Screen::Todos, None).expect("To do is a place");
        assert_eq!(Place::of(Screen::NewFlow, None), None);
        let mut h = History::default();
        for place in [
            on_flow,
            // Deleted since.
            Place::Flow(FlowId(2), None),
            on_tab,
            // A conversation no longer in the flow.
            Place::Flow(FlowId(1), Some(ConversationId(11))),
            todos,
        ] {
            h.visit(place);
        }
        assert_eq!(h.back(there), Some(on_tab));
        assert_eq!(h.back(there), Some(on_flow));
        assert_eq!(h.back(there), None);
        assert_eq!(h.forward(there), Some(on_tab));
        assert_eq!(h.forward(there), Some(todos));
        assert_eq!(h.forward(there), None);
    }

    #[test]
    fn back_and_forward_follow_the_places_visited() {
        let mut h = History::default();
        for place in [1, 2, 3] {
            h.visit(place);
        }
        // The same place twice in a row is one visit.
        h.visit(3);
        assert_eq!(h.back(|_| true), Some(2));
        assert_eq!(h.back(|_| true), Some(1));
        assert_eq!(h.back(|_| true), None);
        assert_eq!(h.forward(|_| true), Some(2));
        // Somewhere new after going back: what was ahead goes.
        h.visit(4);
        assert_eq!(h.forward(|_| true), None);
        assert_eq!(h.back(|_| true), Some(2));
        // A place gone (a deleted flow) is skipped.
        h.visit(5);
        h.visit(6);
        assert_eq!(h.back(|p| *p != 5), Some(2));
        assert_eq!(h.forward(|p| *p != 5), Some(6));
    }

    #[test]
    fn only_the_last_places_are_kept() {
        let mut h = History::default();
        for place in 0..60 {
            h.visit(place);
        }
        let mut oldest = None;
        while let Some(p) = h.back(|_| true) {
            oldest = Some(p);
        }
        assert_eq!(oldest, Some(10));
    }
}
