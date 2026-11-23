//! Leon's own updates (DOCTOR-3): its latest release on GitHub and where an
//! update stands.

/// A release of Leon on GitHub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Its version, without the tag's `v`: "0.4.3".
    pub version: String,
    /// Its GitHub page, with the release notes.
    pub page: String,
    /// The macOS disk image's file name, when the release has one.
    pub dmg: Option<String>,
    /// That file's SHA-256 in hex, as GitHub gives it, when it does.
    pub sha256: Option<String>,
}

/// Where Leon's update stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Update {
    /// Not checked yet.
    #[default]
    Unchecked,
    /// The check failed, and why (e.g. `gh` not logged in).
    Unknown(String),
    /// This Leon is the latest.
    UpToDate,
    /// A newer release.
    Available(Release),
    /// Being downloaded, checked and put in place.
    Installing(Release),
    /// In place: Leon restarts to run it.
    Installed(Release),
    /// Leon quits; the new one starts once it has.
    Restarting(Release),
    /// Installing failed, and why.
    Failed(Release, String),
}

impl Update {
    /// The newer release it is about, if any.
    pub fn release(&self) -> Option<&Release> {
        match self {
            Self::Available(r)
            | Self::Installing(r)
            | Self::Installed(r)
            | Self::Restarting(r)
            | Self::Failed(r, _) => Some(r),
            Self::Unchecked | Self::Unknown(_) | Self::UpToDate => None,
        }
    }
}

/// Whether version `latest` (a tag, with its `v` or not) is newer than
/// `current`: compared number by number, a missing one counting as 0.
pub fn is_newer(latest: &str, current: &str) -> bool {
    numbers(latest) > numbers(current)
}

/// "v0.4.12" → [0, 4, 12]; what follows a number's digits ("-rc1") is left
/// out.
fn numbers(version: &str) -> Vec<u64> {
    let mut parts: Vec<u64> = version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        })
        .collect();
    while parts.last() == Some(&0) {
        parts.pop();
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn a_release_is_newer_number_by_number() {
        let cases = [
            ("v0.4.3", "0.4.2", true),
            ("0.5.0", "0.4.12", true),
            ("v0.4.10", "0.4.9", true),
            ("1.0", "0.9.9", true),
            ("v0.4.2", "0.4.2", false),
            ("0.4", "0.4.0", false),
            ("v0.4.1", "0.4.2", false),
            ("v0.3.9", "0.4.0", false),
        ];
        for (latest, current, newer) in cases {
            assert_eq!(is_newer(latest, current), newer, "{latest} vs {current}");
        }
    }
}
