//! What the user was writing, kept per conversation (CONV-1): the message
//! box is one for every conversation, so on leaving one its text and the
//! long pastes it refers to (CONV-12) are put aside, and given back when it
//! is shown again; another conversation gets its own, or an empty box.

use std::collections::HashMap;

use gpui::{Context, Window};
use leon_domain::ConversationId;

use crate::workspace::Workspace;

/// A message being written: its text, and the pastes its tokens stand for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Draft {
    pub text: String,
    pub pasted: Vec<String>,
}

/// The drafts put aside, by conversation. `None` is the forms starting a
/// flow or a conversation: their text stays in their own box, only their
/// pastes are kept here.
#[derive(Debug, Default)]
pub(crate) struct Drafts(HashMap<Option<ConversationId>, Draft>);

impl Drafts {
    /// Puts aside `leaving`, the draft of `from` (forgotten when empty), and
    /// gives back the one of `to` (empty if none).
    fn switch(
        &mut self,
        from: Option<ConversationId>,
        leaving: Draft,
        to: Option<ConversationId>,
    ) -> Draft {
        if leaving.text.trim().is_empty() && leaving.pasted.is_empty() {
            self.0.remove(&from);
        } else {
            self.0.insert(from, leaving);
        }
        self.0.remove(&to).unwrap_or_default()
    }
}

impl Workspace {
    /// The conversation shown changed from `from` to `to`: the message box
    /// and the pastes become `to`'s draft.
    pub(crate) fn switch_draft(
        &mut self,
        from: Option<ConversationId>,
        to: Option<ConversationId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let leaving = Draft {
            // The message box is a conversation's; a form keeps its own.
            text: from.map_or_else(String::new, |_| {
                crate::workspace::text_of(&self.composer, cx)
            }),
            pasted: std::mem::take(&mut self.pasted),
        };
        let coming = self.drafts.switch(from, leaving, to);
        self.pasted = coming.pasted;
        self.composer
            .update(cx, |state, cx| state.set_value(coming.text, window, cx));
    }
}

#[cfg(test)]
mod tests {
    use leon_domain::ConversationId;

    use super::{Draft, Drafts};

    fn draft(text: &str, pasted: &[&str]) -> Draft {
        Draft {
            text: text.into(),
            pasted: pasted.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    #[test]
    fn each_conversation_gets_back_what_was_written_in_it() {
        let (a, b) = (Some(ConversationId(1)), Some(ConversationId(2)));
        let mut drafts = Drafts::default();
        // Written in a, then b shown: b's box is empty.
        assert_eq!(drafts.switch(a, draft("Fix the", &[]), b), Draft::default());
        // Written in b too, then back to a: a's text, not b's.
        assert_eq!(
            drafts.switch(b, draft("[Pasted #1]", &["log"]), a),
            draft("Fix the", &[])
        );
        // Sent in a (an empty box), then b: b's, with its paste.
        assert_eq!(
            drafts.switch(a, Draft::default(), b),
            draft("[Pasted #1]", &["log"])
        );
        // a's empty draft was forgotten.
        assert_eq!(drafts.switch(b, Draft::default(), a), Draft::default());
        // A form's pastes come back to the form.
        assert_eq!(
            drafts.switch(None, draft("", &["diff"]), a),
            Draft::default()
        );
        assert_eq!(
            drafts.switch(a, Draft::default(), None),
            draft("", &["diff"])
        );
    }
}
