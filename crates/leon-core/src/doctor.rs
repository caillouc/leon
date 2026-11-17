//! The doctor report (like `flutter doctor`): Leon's own facts, then each
//! outside tool, gathered off the core's thread at startup and on demand.

use std::sync::Arc;

use leon_domain::{DoctorCheck, DoctorSection};

use crate::app::Core;
use crate::repo::Internal;

/// Leon's own facts for the doctor: version, config, folders. Given by the
/// binary, which knows them.
pub type About = Arc<dyn Fn() -> Vec<DoctorCheck> + Send + Sync>;

impl Core {
    /// Runs every check in the background; a run already going is enough.
    pub(crate) fn run_doctor(&mut self) {
        if self.state.doctor_running {
            return;
        }
        self.state.doctor_running = true;
        let about = Arc::clone(&self.ports.about);
        let (catalog, tools) = (
            Arc::clone(&self.ports.catalog),
            self.ports.app_tools.clone(),
        );
        let (backends, git, github) = (
            Arc::clone(&self.ports.backends),
            Arc::clone(&self.ports.git),
            Arc::clone(&self.ports.github),
        );
        let tx = self.internal_tx.clone();
        tokio::task::spawn_blocking(move || {
            let mut leon = about();
            leon.extend(tools.map(|t| t.doctor()));
            leon.extend(catalog.doctor());
            let mut sections = vec![DoctorSection {
                title: "Leon".to_owned(),
                checks: leon,
            }];
            sections.extend(backends.doctor());
            sections.push(git.doctor());
            sections.push(github.doctor());
            let _ = tx.send(Internal::Doctor(sections));
        });
    }

    pub(crate) fn doctor_done(&mut self, sections: Vec<DoctorSection>) {
        self.state.doctor_running = false;
        self.state.doctor = Some((self.now_ms(), Arc::new(sections)));
    }
}
