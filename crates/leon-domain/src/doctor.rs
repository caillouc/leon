//! The doctor report: what works, what is missing, what to do about it, like
//! `flutter doctor`.

/// How a check went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    /// Works.
    Ok,
    /// Works, with something to know (an older version, a skipped check).
    Warning,
    /// Does not work until the user does something.
    Problem,
    /// A fact, nothing to judge (a version, a folder).
    Info,
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheck {
    /// How it went.
    pub health: Health,
    /// What was checked, e.g. "Logged in".
    pub name: String,
    /// What was found, e.g. a version or a path.
    pub detail: String,
    /// What to do about it, e.g. a command to run.
    pub fix: Option<String>,
}

impl DoctorCheck {
    /// A check without a fix.
    pub fn new(health: Health, name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            health,
            name: name.into(),
            detail: detail.into(),
            fix: None,
        }
    }

    /// The same check, with what to do about it.
    #[must_use]
    pub fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
}

/// A group of checks: Leon itself, one CLI, git, GitHub…
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorSection {
    /// Title, e.g. "Claude Code".
    pub title: String,
    /// Its checks, in order.
    pub checks: Vec<DoctorCheck>,
}

impl DoctorSection {
    /// The worst health of its checks (`Info` counts as `Ok`).
    pub fn health(&self) -> Health {
        let has = |h| self.checks.iter().any(|c| c.health == h);
        if has(Health::Problem) {
            Health::Problem
        } else if has(Health::Warning) {
            Health::Warning
        } else {
            Health::Ok
        }
    }
}

/// The first `x.y.z` (or `x.y`) in a CLI's `--version` output, e.g.
/// `"2.1.288 (Claude Code)"` → `[2, 1, 288]`.
pub fn version_in(text: &str) -> Option<Vec<u64>> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|word| word.contains('.'))
        .find_map(|word| {
            let parts: Option<Vec<u64>> = word
                .trim_matches('.')
                .split('.')
                .map(|p| p.parse().ok())
                .collect();
            parts.filter(|p| p.len() >= 2)
        })
}

/// Version check of an installed tool against the one Leon was checked with.
pub fn version_check(name: &str, found: &str, checked_with: &str) -> DoctorCheck {
    let detail = format!("{found} (Leon was checked with {checked_with})");
    match (version_in(found), version_in(checked_with)) {
        (Some(have), Some(want)) if have < want => DoctorCheck::new(Health::Warning, name, detail)
            .fix(format!(
                "Update {name}: an older version may lack options Leon uses"
            )),
        (Some(_), Some(_)) => DoctorCheck::new(Health::Ok, name, detail),
        _ => DoctorCheck::new(
            Health::Warning,
            name,
            format!("{found}: version not recognised"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{DoctorCheck, DoctorSection, Health, version_check, version_in};

    #[test]
    fn versions_are_read_from_cli_output() {
        let cases = [
            ("2.1.288 (Claude Code)", Some(vec![2, 1, 288])),
            ("codex-cli 0.160.0", Some(vec![0, 160, 0])),
            ("gh version 2.101.0 (2026-09-15)", Some(vec![2, 101, 0])),
            ("git version 2.54.0 (Apple Git-157)", Some(vec![2, 54, 0])),
            ("no version here", None),
        ];
        for (text, expected) in cases {
            assert_eq!(version_in(text), expected, "{text}");
        }
    }

    #[test]
    fn older_versions_warn() {
        assert_eq!(
            version_check("Codex", "codex-cli 0.150.2", "0.160.0").health,
            Health::Warning
        );
        assert_eq!(
            version_check("Codex", "codex-cli 0.161.0", "0.160.0").health,
            Health::Ok
        );
        assert_eq!(
            version_check("Codex", "codex-cli 0.160.0", "0.160.0").health,
            Health::Ok
        );
        assert_eq!(
            version_check("Codex", "weird", "0.160.0").health,
            Health::Warning
        );
    }

    #[test]
    fn a_section_is_as_bad_as_its_worst_check() {
        let section = |healths: &[Health]| DoctorSection {
            title: "t".into(),
            checks: healths
                .iter()
                .map(|h| DoctorCheck::new(*h, "n", "d"))
                .collect(),
        };
        assert_eq!(section(&[Health::Info, Health::Ok]).health(), Health::Ok);
        assert_eq!(
            section(&[Health::Ok, Health::Warning]).health(),
            Health::Warning
        );
        assert_eq!(
            section(&[Health::Warning, Health::Problem]).health(),
            Health::Problem
        );
    }
}
