//! What each CLI says of its plan (DOCTOR-2, USE-1): its windows and the
//! credits its account has left, with when it said it. Saved, so it shows
//! from launch until the CLI says again; a CLI that can tell without a turn
//! (Codex) is asked when Doctor runs.

use std::sync::Arc;

use leon_domain::{BackendKind, PlanUsage};

use crate::app::{Core, blocking};
use crate::ports::PlanSaid;
use crate::repo::Internal;

impl Core {
    /// What the CLIs said before Leon started.
    pub(crate) fn load_plan_usage(&mut self) {
        let storage = Arc::clone(&self.ports.storage);
        self.state.usage =
            Arc::new(Self::report(blocking(|| storage.plan_usage())).unwrap_or_default());
    }

    /// `backend` says `(windows, credits)` of its plan now: it replaces what
    /// it said before, and is saved.
    pub(crate) fn plan_said(&mut self, backend: BackendKind, (windows, credits): PlanSaid) {
        let now = self.now_ms();
        // A model's own window, said on its turns only, stays meanwhile.
        let before = self
            .state
            .usage
            .iter()
            .find(|u| u.backend == backend)
            .map_or(&[][..], |u| u.windows.as_slice());
        let windows = leon_domain::with_model_windows(windows, before, now);
        let usage = PlanUsage {
            backend,
            windows,
            credits,
            at: now,
        };
        let all = Arc::make_mut(&mut self.state.usage);
        all.retain(|u| u.backend != backend);
        all.push(usage.clone());
        all.sort_by_key(|u| BackendKind::ALL.iter().position(|b| *b == u.backend));
        let storage = Arc::clone(&self.ports.storage);
        Self::report(blocking(|| storage.save_plan_usage(&usage)));
    }

    /// Asks each CLI that can tell its plan without a turn, in the
    /// background.
    pub(crate) fn ask_plan_usage(&self) {
        for backend in BackendKind::ALL {
            let backends = Arc::clone(&self.ports.backends);
            let tx = self.internal_tx.clone();
            crate::jobs::spawn(move || {
                if let Some(said) = backends.plan_usage(backend) {
                    let _ = tx.send(Internal::PlanUsage(backend, said));
                }
            });
        }
    }
}
