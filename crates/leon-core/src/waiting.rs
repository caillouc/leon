//! What waits for a conversation's turn to end — the user's queued message
//! (CONV-19), other agents' messages (FLOW-1) — saved at each change so a
//! restart loses none of it (I30), and sent once the turn ends.

use std::sync::Arc;

use leon_domain::ConversationId;

use crate::app::{Core, blocking};
use crate::ports::{Image, WaitingFor};

impl Core {
    /// The user wrote to conversation `id` while it answers: the message
    /// waits at place `at` (an edited one back where it was), else after
    /// those already waiting; its pasted images are saved now so a restart
    /// keeps them.
    pub(crate) fn queue(
        &mut self,
        id: ConversationId,
        at: Option<usize>,
        text: &str,
        images: &[Image],
    ) {
        let backends = Arc::clone(&self.ports.backends);
        let images: Vec<Image> = images
            .iter()
            .map(|image| match image {
                Image::Pasted { .. } => Self::report(blocking(|| backends.save_image(id, image)))
                    .map_or_else(|| image.clone(), Image::File),
                Image::File(_) => image.clone(),
            })
            .collect();
        let Some(s) = self.state.conversations.get_mut(&id) else {
            return;
        };
        let at = at.map_or(s.queued.len(), |at| at.min(s.queued.len()));
        s.queued.insert(at, (text.trim().to_owned(), images));
        self.save_waiting(id);
    }

    /// Sends conversation `id` what waits for it: the user's queued
    /// messages first, all as one, else the next agent's.
    pub(crate) fn send_next(&mut self, id: ConversationId) {
        let Some(s) = self.state.conversations.get_mut(&id) else {
            return;
        };
        let queued = std::mem::take(&mut s.queued);
        let next = if queued.is_empty() {
            s.inbox.pop_front()
        } else {
            None
        };
        if queued.is_empty() && next.is_none() {
            return;
        }
        self.save_waiting(id);
        if let Some(w) = next {
            self.message(w.from, id, w.text, w.sent);
        } else {
            let (texts, images): (Vec<String>, Vec<Vec<Image>>) = queued.into_iter().unzip();
            self.send(id, &texts.join("\n\n"), &images.concat());
        }
    }

    /// Saves what waits for conversation `id` now.
    pub(crate) fn save_waiting(&self, id: ConversationId) {
        let Some(s) = self.state.conversations.get(&id) else {
            return;
        };
        let waiting = WaitingFor {
            queued: s
                .queued
                .iter()
                .map(|(text, images)| {
                    let files = images.iter().filter_map(|image| match image {
                        Image::File(path) => Some(path.clone()),
                        Image::Pasted { .. } => None,
                    });
                    (text.clone(), files.collect())
                })
                .collect(),
            inbox: s.inbox.iter().cloned().collect(),
        };
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.save_waiting(id, &waiting)));
    }

    /// At startup: each conversation gets back what waited for it.
    pub(crate) fn load_waiting(&mut self) {
        let storage = Arc::clone(&self.ports.storage);
        let saved = Self::report(blocking(|| storage.waiting())).unwrap_or_default();
        for (id, waiting) in saved {
            let Some(s) = self.state.conversations.get_mut(&id) else {
                continue;
            };
            s.queued = waiting
                .queued
                .into_iter()
                .map(|(text, files)| (text, files.into_iter().map(Image::File).collect()))
                .collect();
            s.inbox = waiting.inbox.into();
        }
    }

    /// Once Leon is back with the CLIs still working: a conversation idle
    /// by then gets what waits for it; one in a turn, when it ends.
    pub(crate) fn send_waiting(&mut self) {
        let idle: Vec<ConversationId> = self
            .state
            .conversations
            .values()
            .filter(|s| crate::agents::ready(s.status))
            .filter(|s| !s.queued.is_empty() || !s.inbox.is_empty())
            .map(|s| s.conversation.id)
            .collect();
        for id in idle {
            self.send_next(id);
        }
    }
}
