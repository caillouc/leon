//! The models each CLI offers (I31, CONV-4), asked at launch, or, for
//! Copilot CLI, listed by its conversations' sessions: the pickers show the
//! list built into Leon until a CLI answers, then the CLI's own.

use std::sync::Arc;

use leon_domain::{BackendKind, ConversationId};

use crate::app::Core;
use crate::ports::ModelOption;
use crate::repo::Internal;

impl Core {
    /// Asks each CLI, in the background, which models it offers.
    pub(crate) fn ask_models(&self) {
        for backend in BackendKind::ALL {
            let backends = Arc::clone(&self.ports.backends);
            let tx = self.internal_tx.clone();
            crate::jobs::spawn(move || {
                if let Some(models) = backends.models(backend).filter(|m| !m.is_empty()) {
                    let _ = tx.send(Internal::Models(backend, models));
                }
            });
        }
    }

    /// Conversation `id`'s session listed its CLI's models (Copilot CLI):
    /// the pickers offer them.
    pub(crate) fn session_models(&mut self, id: ConversationId, models: Vec<ModelOption>) {
        let backend = self
            .state
            .conversations
            .get(&id)
            .map(|s| s.conversation.settings.backend);
        if let Some(backend) = backend.filter(|_| !models.is_empty()) {
            self.models_listed(backend, models);
        }
    }

    /// A CLI listed its models: the pickers offer them.
    pub(crate) fn models_listed(&mut self, backend: BackendKind, models: Vec<ModelOption>) {
        let mut options = (*self.state.options).clone();
        options.entry(backend).or_default().models = models;
        self.state.options = Arc::new(options);
    }
}
