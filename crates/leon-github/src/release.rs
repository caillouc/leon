//! Leon's own releases (DOCTOR-3, I26): the latest one, read with `gh api
//! repos/<owner/name>/releases/latest` (GitHub leaves out drafts and
//! pre-releases), and one of its files downloaded with `gh release download`
//! (gh 2.101.0, both run on the v0.4.2 release, 2026-11-23).

use std::path::Path;

use leon_domain::Release;
use serde_json::Value;

/// `gh api` for the latest release of `owner/name`.
pub(crate) fn latest(repo: &str) -> Vec<String> {
    vec!["api".into(), format!("repos/{repo}/releases/latest")]
}

/// `gh release download` of file `name` of release `version` into `dir`,
/// replacing a file left there.
pub(crate) fn download(repo: &str, version: &str, name: &str, dir: &Path) -> Vec<String> {
    vec![
        "release".into(),
        "download".into(),
        format!("v{version}"),
        "--repo".into(),
        repo.into(),
        "--pattern".into(),
        name.into(),
        "--dir".into(),
        dir.to_string_lossy().into_owned(),
        "--clobber".into(),
    ]
}

/// The release in `gh api`'s answer: its version from its tag, its page, its
/// `.dmg` and that file's SHA-256 (`digest`, "sha256:<hex>").
pub(crate) fn parse(json: &str) -> Result<Release, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tag = v["tag_name"].as_str().ok_or("no tag in GitHub's answer")?;
    let dmg = v["assets"].as_array().into_iter().flatten().find(|a| {
        a["name"].as_str().is_some_and(|n| {
            Path::new(n)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dmg"))
        })
    });
    Ok(Release {
        version: tag.trim_start_matches('v').to_owned(),
        page: v["html_url"].as_str().unwrap_or_default().to_owned(),
        dmg: dmg.and_then(|a| a["name"].as_str()).map(str::to_owned),
        sha256: dmg
            .and_then(|a| a["digest"].as_str())
            .and_then(|d| d.strip_prefix("sha256:"))
            .map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use leon_domain::Release;

    use super::parse;

    #[test]
    fn the_latest_release_gives_its_version_page_and_disk_image() {
        let json = include_str!("../fixtures/gh/2.101.0/release-latest.json");
        assert_eq!(
            parse(json),
            Ok(Release {
                version: "0.4.2".into(),
                page: "https://github.com/acme/leon/releases/tag/v0.4.2".into(),
                dmg: Some("Leon-0.4.2.dmg".into()),
                sha256: Some(
                    "92f5c404e90cec85c6d79519f6c3f689b4a3fd7efea17a02232e631cd457833d".into()
                ),
            })
        );
    }

    /// A release without a disk image (only the Linux build, say) still
    /// says its version; one without a tag is no answer.
    #[test]
    fn a_release_without_a_disk_image_has_none() {
        let release = parse(r#"{"tag_name":"v1.0.0","html_url":"u","assets":[]}"#);
        assert_eq!(
            release.map(|r| (r.version, r.dmg)),
            Ok(("1.0.0".into(), None))
        );
        assert!(parse(r#"{"message":"Not Found"}"#).is_err());
    }
}
