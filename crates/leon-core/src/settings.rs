//! The user's settings (SETTINGS-1): saved to the config file through the
//! [`ConfigFile`](crate::ports::ConfigFile) port, then used at once.

use std::sync::Arc;

use leon_domain::{FoldPattern, Settings};

use crate::app::Core;

impl Core {
    /// Saves `settings`, then uses them. Source folders are read at start, so
    /// a change to them applies the next time Leon starts.
    pub(crate) fn save_settings(&mut self, settings: Settings) {
        if settings == self.state.settings {
            return;
        }
        let config = Arc::clone(&self.ports.config);
        match tokio::task::block_in_place(|| config.save(&settings)) {
            Ok(()) => {
                self.state.settings = settings;
                self.apply_fold();
            }
            Err(error) => self.flash(format!("Could not save the settings: {error}")),
        }
    }

    /// `diff.fold` as patterns (GIT-4).
    pub(crate) fn apply_fold(&mut self) {
        let fold = self
            .state
            .settings
            .diff_fold
            .iter()
            .filter_map(|p| FoldPattern::parse(p))
            .collect();
        self.state.fold = Arc::new(fold);
    }
}
