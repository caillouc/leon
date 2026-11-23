//! Leon putting a newer release in its own place (DOCTOR-3, I26), on macOS:
//! the `.dmg` checked against GitHub's SHA-256 (`shasum`), mounted read-only
//! (`hdiutil`), the app in it checked — its signature (`codesign --verify`),
//! its Team ID the running Leon's (`codesign -dv`), notarized (`spctl`) —
//! then copied (`ditto`) next to the running bundle and swapped in by two
//! renames. Each tool run by hand on the v0.4.2 release, 2026-11-23. The
//! tools are macOS's own, run by full path, never through a shell; only the
//! relaunch's wait for this process to end is a shell line.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use leon_core::ports::{PortError, Updater};
use leon_domain::Release;

const CONTEXT: &str = "installing Leon";

/// Leon's releases: the repository in its `Cargo.toml`, `owner/name`.
fn releases_repo() -> &'static str {
    env!("CARGO_PKG_REPOSITORY").trim_start_matches("https://github.com/")
}

/// [`Updater`] for the Leon running from `bundle`, downloading into
/// `downloads` (in Leon's data folder).
pub(crate) struct SelfUpdate {
    bundle: Option<PathBuf>,
    downloads: PathBuf,
}

impl SelfUpdate {
    /// For the Leon whose program is `exe`, with its data in `data`.
    pub(crate) fn new(exe: &Path, data: &Path) -> Self {
        let exe = exe.canonicalize().unwrap_or_else(|_| exe.to_owned());
        Self {
            bundle: cfg!(target_os = "macos").then(|| bundle_of(&exe)).flatten(),
            downloads: data.join("updates"),
        }
    }
}

/// The app bundle `exe` runs from: `…/Leon.app` for
/// `…/Leon.app/Contents/MacOS/leon`.
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension() == Some(OsStr::new("app")))
        .map(Path::to_owned)
}

impl Updater for SelfUpdate {
    fn repo(&self) -> &'static str {
        releases_repo()
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn can_install(&self) -> bool {
        self.bundle.is_some()
    }

    fn downloads(&self) -> Result<PathBuf, PortError> {
        remove(&self.downloads)?;
        std::fs::create_dir_all(&self.downloads).map_err(|e| PortError::new(CONTEXT, e))?;
        Ok(self.downloads.clone())
    }

    fn install(&self, file: &Path, release: &Release) -> Result<(), PortError> {
        let bundle = self
            .bundle
            .as_deref()
            .ok_or_else(|| fail("Leon does not run from its app bundle"))?;
        let expected = release
            .sha256
            .as_deref()
            .ok_or_else(|| fail("GitHub gives no checksum for it"))?;
        let sum = run("/usr/bin/shasum", [os("-a"), os("256"), file.as_os_str()])?;
        if !sum
            .split_whitespace()
            .next()
            .is_some_and(|s| s.eq_ignore_ascii_case(expected))
        {
            return Err(fail("its checksum is not the one GitHub gives"));
        }
        let mount = self.downloads.join("mounted");
        run(
            "/usr/bin/hdiutil",
            [
                os("attach"),
                os("-nobrowse"),
                os("-readonly"),
                os("-noautoopen"),
                os("-mountpoint"),
                mount.as_os_str(),
                file.as_os_str(),
            ],
        )?;
        let placed = check_and_place(&mount.join("Leon.app"), bundle);
        if let Err(error) = run("/usr/bin/hdiutil", [os("detach"), mount.as_os_str()]) {
            tracing::warn!(%error, "the downloaded Leon stays mounted");
        }
        if let Err(error) = remove(&self.downloads) {
            tracing::warn!(%error, "the downloaded Leon stays on disk");
        }
        placed
    }

    fn relaunch(&self) -> Result<(), PortError> {
        use std::os::unix::process::CommandExt as _;
        let bundle = self
            .bundle
            .as_deref()
            .ok_or_else(|| fail("Leon does not run from its app bundle"))?;
        // Waits for this Leon to end, then opens the bundle: one Leon at a
        // time holds the agents' socket.
        Command::new("/bin/sh")
            .arg("-c")
            .arg(r#"while kill -0 "$0" 2>/dev/null; do sleep 0.2; done; exec /usr/bin/open "$1""#)
            .arg(std::process::id().to_string())
            .arg(bundle)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(drop)
            .map_err(|e| PortError::new("restarting Leon", e))
    }
}

/// Checks `app` is Leon's own — validly signed, by the running Leon's
/// developer, notarized — then puts it in place of `bundle`.
fn check_and_place(app: &Path, bundle: &Path) -> Result<(), PortError> {
    let path = app.as_os_str();
    run(
        "/usr/bin/codesign",
        [os("--verify"), os("--deep"), os("--strict"), path],
    )
    .map_err(|e| fail(format!("its signature is not valid: {}", e.message)))?;
    let (theirs, ours) = (team(app)?, team(bundle)?);
    if theirs != ours {
        return Err(fail(format!(
            "it is signed by team {theirs}, not Leon's ({ours})"
        )));
    }
    run(
        "/usr/sbin/spctl",
        [os("--assess"), os("--type"), os("execute"), path],
    )
    .map_err(|e| {
        fail(format!(
            "macOS does not accept it as notarized: {}",
            e.message
        ))
    })?;
    replace(app, bundle)
}

/// The Team ID `app` is signed with.
fn team(app: &Path) -> Result<String, PortError> {
    // `codesign -dv` writes what it shows on stderr.
    let out = output("/usr/bin/codesign", [os("-dv"), app.as_os_str()])?;
    team_in(&String::from_utf8_lossy(&out.stderr))
        .ok_or_else(|| fail(format!("{} is not signed by a developer", app.display())))
}

/// `TeamIdentifier=…` in `codesign -dv`'s report; "not set" is none.
fn team_in(report: &str) -> Option<String> {
    report
        .lines()
        .find_map(|l| l.strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|t| !t.is_empty() && *t != "not set")
        .map(str::to_owned)
}

/// Copies `app` next to `bundle`, then swaps them by renames: if the second
/// rename fails, the old one goes back.
fn replace(app: &Path, bundle: &Path) -> Result<(), PortError> {
    let (Some(parent), Some(name)) = (bundle.parent(), bundle.file_name()) else {
        return Err(fail("Leon's app bundle has no folder"));
    };
    let name = name.to_string_lossy();
    let (staged, old) = (
        parent.join(format!(".{name}.new")),
        parent.join(format!(".{name}.old")),
    );
    remove(&staged)?;
    remove(&old)?;
    run("/usr/bin/ditto", [app.as_os_str(), staged.as_os_str()])?;
    let moved = |e: std::io::Error| fail(format!("{}: {e}", parent.display()));
    std::fs::rename(bundle, &old).map_err(moved)?;
    if let Err(error) = std::fs::rename(&staged, bundle) {
        return Err(match std::fs::rename(&old, bundle) {
            Ok(()) => moved(error),
            Err(back) => fail(format!(
                "{error}; the old Leon is at {}: {back}",
                old.display()
            )),
        });
    }
    if let Err(error) = remove(&old) {
        tracing::warn!(%error, "the replaced Leon stays next to the new one");
    }
    Ok(())
}

/// Removes a file or folder, if there.
fn remove(path: &Path) -> Result<(), PortError> {
    let removed = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match removed {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(fail(format!("{}: {e}", path.display())))
        }
        _ => Ok(()),
    }
}

fn os(arg: &str) -> &OsStr {
    OsStr::new(arg)
}

fn fail(message: impl std::fmt::Display) -> PortError {
    PortError::new(CONTEXT, message)
}

/// Runs `program args…` and returns what it printed; failing, what it said.
fn run<'a>(program: &str, args: impl IntoIterator<Item = &'a OsStr>) -> Result<String, PortError> {
    let out = output(program, args)?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    Err(fail(format!("{program}: {}", said.trim())))
}

fn output<'a>(
    program: &str,
    args: impl IntoIterator<Item = &'a OsStr>,
) -> Result<std::process::Output, PortError> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| fail(format!("{program}: {e}")))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{bundle_of, releases_repo, team_in};

    #[test]
    fn releases_come_from_leons_repository() {
        let repo = releases_repo();
        assert_eq!(repo.split('/').count(), 2, "{repo}");
    }

    #[test]
    fn leon_runs_from_the_bundle_around_its_program() {
        let cases = [
            (
                "/Applications/Leon.app/Contents/MacOS/leon",
                Some("/Applications/Leon.app"),
            ),
            ("/Users/me/leon/target/debug/leon", None),
        ];
        for (exe, bundle) in cases {
            assert_eq!(
                bundle_of(Path::new(exe)),
                bundle.map(PathBuf::from),
                "{exe}"
            );
        }
    }

    #[test]
    fn the_team_comes_from_codesigns_report() {
        let signed = "Identifier=dev.leon.desktop\nTeamIdentifier=ABCDE12345\nSealed Resources";
        assert_eq!(team_in(signed).as_deref(), Some("ABCDE12345"));
        assert_eq!(team_in("TeamIdentifier=not set\n"), None);
        assert_eq!(team_in("code object is not signed at all"), None);
    }

    /// The whole update, for real, into a temporary copy of Leon: the
    /// latest release read and downloaded with `gh`, then checked and put in
    /// place of a copy of its own app.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "downloads Leon's latest release from GitHub with gh"]
    fn the_latest_release_installs_over_a_copy_of_leon() {
        use leon_core::ports::{GitHub, Updater};
        let dir = tempfile::tempdir().unwrap();
        let github = leon_github::GhCli::default();
        let bundle = dir.path().join("Applications/Leon.app");
        let updater = super::SelfUpdate {
            bundle: Some(bundle.clone()),
            downloads: dir.path().join("updates"),
        };
        let release = github.latest_release(updater.repo()).unwrap();
        let name = release.dmg.clone().unwrap();
        // The "running" Leon: the release's own app, copied out.
        let first = github
            .download_release(
                updater.repo(),
                &release.version,
                &name,
                &updater.downloads().unwrap(),
            )
            .unwrap();
        let mount = dir.path().join("first");
        super::run(
            "/usr/bin/hdiutil",
            [
                super::os("attach"),
                super::os("-nobrowse"),
                super::os("-readonly"),
                super::os("-mountpoint"),
                mount.as_os_str(),
                first.as_os_str(),
            ],
        )
        .unwrap();
        std::fs::create_dir_all(bundle.parent().unwrap()).unwrap();
        super::run(
            "/usr/bin/ditto",
            [mount.join("Leon.app").as_os_str(), bundle.as_os_str()],
        )
        .unwrap();
        super::run("/usr/bin/hdiutil", [super::os("detach"), mount.as_os_str()]).unwrap();
        let file = github
            .download_release(
                updater.repo(),
                &release.version,
                &name,
                &updater.downloads().unwrap(),
            )
            .unwrap();
        updater.install(&file, &release).unwrap();
        super::run(
            "/usr/bin/codesign",
            [
                super::os("--verify"),
                super::os("--deep"),
                super::os("--strict"),
                bundle.as_os_str(),
            ],
        )
        .unwrap();
        assert!(!dir.path().join("updates").exists());
    }

    /// The new app takes the old one's place; nothing else stays.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_new_app_takes_the_running_ones_place() {
        use super::replace;
        let dir = tempfile::tempdir().unwrap();
        let app = |root: &Path, version: &str| {
            let contents = root.join("Contents");
            std::fs::create_dir_all(&contents).unwrap();
            std::fs::write(contents.join("version"), version).unwrap();
        };
        let (new, bundle) = (
            dir.path().join("mounted/Leon.app"),
            dir.path().join("Applications/Leon.app"),
        );
        app(&new, "0.5.0");
        app(&bundle, "0.4.2");
        replace(&new, &bundle).unwrap();
        let version = std::fs::read_to_string(bundle.join("Contents/version")).unwrap();
        assert_eq!(version, "0.5.0");
        let left: Vec<_> = std::fs::read_dir(dir.path().join("Applications"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["Leon.app"]);
    }
}
