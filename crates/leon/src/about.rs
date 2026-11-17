//! Leon's own facts for the doctor: version, config, data folder, `PATH`,
//! notifications.

use std::path::PathBuf;
use std::sync::Arc;

use leon_core::About;
use leon_domain::{DoctorCheck, Health};

/// What the binary knows at start.
pub(crate) struct Facts {
    pub config: Option<PathBuf>,
    pub config_warnings: Vec<String>,
    pub data: PathBuf,
    pub exe: PathBuf,
    pub login_path: fn() -> Option<&'static std::ffi::OsString>,
}

pub(crate) fn about(facts: Facts) -> About {
    Arc::new(move || {
        let mut checks = vec![DoctorCheck::new(
            Health::Info,
            "Version",
            format!(
                "Leon {} ({}, {})",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
        )];
        checks.push(match &facts.config {
            None => DoctorCheck::new(Health::Warning, "Config", "no home folder: defaults used"),
            Some(path) if !path.exists() => DoctorCheck::new(
                Health::Info,
                "Config",
                format!("{} (not created: defaults used)", path.display()),
            ),
            Some(path) if facts.config_warnings.is_empty() => {
                DoctorCheck::new(Health::Ok, "Config", path.display().to_string())
            }
            Some(path) => DoctorCheck::new(
                Health::Warning,
                "Config",
                format!("{}: {}", path.display(), facts.config_warnings.join("; ")),
            )
            .fix("Fix these keys in the config file; Leon ignores them meanwhile"),
        });
        checks.push(DoctorCheck::new(
            Health::Info,
            "Data folder",
            facts.data.display().to_string(),
        ));
        checks.push(match (facts.login_path)() {
            Some(_) => DoctorCheck::new(Health::Ok, "PATH", "read from your login shell"),
            None => DoctorCheck::new(
                Health::Warning,
                "PATH",
                "your login shell's PATH could not be read",
            )
            .fix("If a CLI shows as not installed, start Leon from a terminal"),
        });
        checks.push(notifications(&facts.exe));
        checks
    })
}

/// macOS shows notifications only for apps: Leon must run from `Leon.app`
/// (gpui logs "system notifications disabled: not running from an app
/// bundle" otherwise; seen on 2026-11-17).
fn notifications(exe: &std::path::Path) -> DoctorCheck {
    let in_bundle = exe.to_string_lossy().contains(".app/Contents/MacOS/");
    if cfg!(target_os = "macos") && !in_bundle {
        DoctorCheck::new(
            Health::Warning,
            "Notifications",
            "off: Leon is not running as an app (Leon.app)",
        )
        .fix("Run Leon from its app bundle to get desktop notifications")
    } else {
        DoctorCheck::new(Health::Ok, "Notifications", "on")
    }
}
