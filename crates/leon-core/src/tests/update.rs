//! Leon's own updates (DOCTOR-3): found when Doctor runs, put in place on
//! the user's word, then Leon starts again.

use std::sync::atomic::{AtomicBool, Ordering};

use leon_domain::{Release, Update};

use super::*;
use crate::ports::Updater;

/// The release the fake GitHub has.
pub(super) fn release() -> Release {
    Release {
        version: "0.5.0".into(),
        page: "https://github.com/acme/leon/releases/tag/v0.5.0".into(),
        dmg: Some("Leon-0.5.0.dmg".into()),
        sha256: Some("ab12".into()),
    }
}

pub(super) struct FakeUpdater {
    version: &'static str,
    can_install: bool,
    /// Why installing fails, if it does.
    fails: Option<&'static str>,
    installed: Mutex<Vec<PathBuf>>,
    relaunched: AtomicBool,
}

impl Default for FakeUpdater {
    fn default() -> Self {
        Self {
            version: "0.4.2",
            can_install: true,
            fails: None,
            installed: Mutex::new(Vec::new()),
            relaunched: AtomicBool::new(false),
        }
    }
}

impl Updater for FakeUpdater {
    fn repo(&self) -> &'static str {
        "acme/leon"
    }
    fn version(&self) -> &str {
        self.version
    }
    fn can_install(&self) -> bool {
        self.can_install
    }
    fn downloads(&self) -> Result<PathBuf, PortError> {
        Ok(PathBuf::from("/data/updates"))
    }
    fn install(&self, file: &Path, release: &Release) -> Result<(), PortError> {
        assert_eq!(release.version, "0.5.0");
        self.installed.lock().unwrap().push(file.to_owned());
        self.fails
            .map_or(Ok(()), |why| Err(PortError::new("installing Leon", why)))
    }
    fn relaunch(&self) -> Result<(), PortError> {
        self.relaunched.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// A core with `updater`, Doctor run: it has asked for the latest release.
async fn checked(updater: &Arc<FakeUpdater>) -> CoreHandle {
    let core = start(
        &tokio::runtime::Handle::current(),
        Ports {
            updater: Arc::clone(updater) as Arc<dyn Updater>,
            ..ports(
                Arc::new(FakeStorage::default()),
                Arc::new(FakeBackends::default()),
            )
        },
        Settings::default(),
    );
    core.send(Intent::RunDoctor);
    until(&core, "checked", |s| s.leon.update != Update::Unchecked).await;
    core
}

#[tokio::test(flavor = "multi_thread")]
async fn doctor_says_when_a_newer_leon_is_out() {
    for (running, expected) in [
        ("0.4.2", Update::Available(release())),
        ("0.5.0", Update::UpToDate),
    ] {
        let updater = Arc::new(FakeUpdater {
            version: running,
            ..FakeUpdater::default()
        });
        let core = checked(&updater).await;
        assert_eq!(core.snapshot().leon.update, expected, "running {running}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_update_is_put_in_place_then_leon_starts_again() {
    let updater = Arc::new(FakeUpdater::default());
    let core = checked(&updater).await;
    core.send(Intent::InstallUpdate);
    until(&core, "installed", |s| {
        s.leon.update == Update::Installed(release())
    })
    .await;
    assert_eq!(
        *updater.installed.lock().unwrap(),
        [PathBuf::from("/data/updates/Leon-0.5.0.dmg")]
    );
    // A check meanwhile does not undo it.
    core.send(Intent::RunDoctor);
    core.send(Intent::RestartLeon);
    until(&core, "restarting", |s| {
        s.leon.update == Update::Restarting(release())
    })
    .await;
    assert!(updater.relaunched.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_update_says_why_and_can_be_tried_again() {
    let updater = Arc::new(FakeUpdater {
        fails: Some("the signature is not Leon's"),
        ..FakeUpdater::default()
    });
    let core = checked(&updater).await;
    core.send(Intent::InstallUpdate);
    let s = until(&core, "failed", |s| {
        matches!(s.leon.update, Update::Failed(..))
    })
    .await;
    assert_eq!(
        s.leon.update,
        Update::Failed(
            release(),
            "installing Leon: the signature is not Leon's".into()
        )
    );
    core.send(Intent::InstallUpdate);
    until(&core, "tried again", |_| {
        updater.installed.lock().unwrap().len() == 2
    })
    .await;
}

/// Where Leon cannot put a release in its own place, only the link is
/// offered: asking to install does nothing.
#[tokio::test(flavor = "multi_thread")]
async fn without_an_app_bundle_nothing_is_installed() {
    let updater = Arc::new(FakeUpdater {
        can_install: false,
        ..FakeUpdater::default()
    });
    let core = checked(&updater).await;
    core.send(Intent::InstallUpdate);
    core.send(Intent::RestartLeon);
    core.send(Intent::RunDoctor);
    let s = until(&core, "doctor", |s| s.doctor.is_some()).await;
    assert_eq!(s.leon.update, Update::Available(release()));
    assert!(updater.installed.lock().unwrap().is_empty());
}
