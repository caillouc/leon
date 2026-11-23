//! Leon's own updates (DOCTOR-3, I26): GitHub is asked for its latest
//! release when Doctor runs (at launch, on "Run again") and once a day; a
//! newer one is downloaded, checked and put in place on the user's word, then
//! Leon starts again on theirs.

use std::sync::Arc;

use leon_domain::{Release, Update, is_newer};

use crate::app::Core;
use crate::repo::Internal;

/// How often Leon looks for a newer release on its own.
const EVERY_MS: i64 = 24 * 60 * 60 * 1000;

impl Core {
    /// Asks GitHub for Leon's latest release in the background: `now`, or
    /// when the last ask is a day old.
    pub(crate) fn check_update(&mut self, now: bool) {
        let at = self.now_ms();
        let recent = self
            .update_checked
            .is_some_and(|last| at.saturating_sub(last) < EVERY_MS);
        if (recent && !now) || self.updating() {
            return;
        }
        self.update_checked = Some(at);
        let (github, updater) = (
            Arc::clone(&self.ports.github),
            Arc::clone(&self.ports.updater),
        );
        let tx = self.internal_tx.clone();
        crate::jobs::spawn(move || {
            let release = github
                .latest_release(updater.repo())
                .map_err(|e| e.to_string());
            let _ = tx.send(Internal::Release(release));
        });
    }

    /// What GitHub said of the latest release.
    pub(crate) fn release_read(&mut self, release: Result<Release, String>) {
        if self.updating() {
            return;
        }
        let current = self.ports.updater.version();
        self.state.leon.update = match release {
            Ok(r) if is_newer(&r.version, current) => Update::Available(r),
            Ok(_) => Update::UpToDate,
            Err(error) => Update::Unknown(error),
        };
    }

    /// Downloads the newer release, checks it and puts it in place, in the
    /// background; again after a failure.
    pub(crate) fn install_update(&mut self) {
        let release = match &self.state.leon.update {
            Update::Available(r) | Update::Failed(r, _) => r.clone(),
            Update::Unchecked
            | Update::Unknown(_)
            | Update::UpToDate
            | Update::Installing(_)
            | Update::Installed(_)
            | Update::Restarting(_) => return,
        };
        if !self.ports.updater.can_install() {
            return;
        }
        self.state.leon.update = Update::Installing(release.clone());
        let (github, updater) = (
            Arc::clone(&self.ports.github),
            Arc::clone(&self.ports.updater),
        );
        let tx = self.internal_tx.clone();
        crate::jobs::spawn(move || {
            let installed = (|| {
                let name = release
                    .dmg
                    .as_deref()
                    .ok_or("this release has no macOS disk image")?;
                let dir = updater.downloads().map_err(|e| e.to_string())?;
                let file = github
                    .download_release(updater.repo(), &release.version, name, &dir)
                    .map_err(|e| e.to_string())?;
                updater.install(&file, &release).map_err(|e| e.to_string())
            })();
            let _ = tx.send(Internal::Installed(installed));
        });
    }

    /// The newer Leon is in place, or why not.
    pub(crate) fn update_installed(&mut self, result: Result<(), String>) {
        let Update::Installing(release) = std::mem::take(&mut self.state.leon.update) else {
            return;
        };
        self.state.leon.update = match result {
            Ok(()) => Update::Installed(release),
            Err(error) => Update::Failed(release, error),
        };
    }

    /// Starts the Leon just installed once this one quits, then has the UI
    /// quit (`Update::Restarting`).
    pub(crate) fn restart(&mut self) {
        let Update::Installed(release) = &self.state.leon.update else {
            return;
        };
        self.state.leon.update = match self.ports.updater.relaunch() {
            Ok(()) => Update::Restarting(release.clone()),
            Err(error) => Update::Failed(release.clone(), error.to_string()),
        };
    }

    /// An update is being put in place, or is: a check must not undo it.
    fn updating(&self) -> bool {
        matches!(
            self.state.leon.update,
            Update::Installing(_) | Update::Installed(_) | Update::Restarting(_)
        )
    }
}
