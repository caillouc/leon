//! Back and forward through the places the user opened (NAV-12): a flow on
//! its Flow tab or on one conversation's tab, in the order visited, like a
//! browser's history.

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

    /// The place the user is at, if `there` says it still is.
    pub(crate) fn current(&self, there: impl Fn(&T) -> bool) -> Option<T> {
        self.places.get(self.at).filter(|p| there(p)).cloned()
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
    use super::History;

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
        assert_eq!(h.current(|_| true), Some(2));
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
