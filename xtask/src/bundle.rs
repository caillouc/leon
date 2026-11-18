//! `cargo xtask bundle [--install]`: builds Leon.app (macOS) from the release
//! binary, the icon in `assets/` and an Info.plist, signs it, and with
//! `--install` moves it to /Applications.
//!
//! Signed with the code-signing certificate named by `LEON_SIGN_IDENTITY`,
//! else an Apple developer certificate or one named "Leon Local" when the
//! keychain has it, else ad hoc. macOS
//! ties folder permissions (Documents, Downloads…) to an ad-hoc app's exact
//! build, so it asks again after each rebuild; a certificate keeps them.

use std::path::{Path, PathBuf};
use std::process::Command;

const PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Leon</string>
  <key>CFBundleDisplayName</key><string>Leon</string>
  <key>CFBundleIdentifier</key><string>dev.leon.desktop</string>
  <key>CFBundleExecutable</key><string>leon</string>
  <key>CFBundleIconFile</key><string>leon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>VERSION</string>
  <key>CFBundleVersion</key><string>VERSION</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
"#;

pub(crate) fn bundle(install: bool) -> bool {
    if !cfg!(target_os = "macos") {
        eprintln!("bundle: only macOS has an app bundle; on Linux run target/release/leon");
        return false;
    }
    if !crate::cargo(&["build", "-p", "leon", "--release", "--locked"]) {
        return false;
    }
    let root = workspace_root();
    let app = root.join("target/release/Leon.app");
    match assemble(&root, &app) {
        Ok(()) => println!("bundle: {}", app.display()),
        Err(error) => {
            eprintln!("bundle: {error}");
            return false;
        }
    }
    let identity = signing_identity();
    if identity == "-" {
        println!(
            "bundle: signed ad hoc; macOS will ask again for folder access. Add your \
             Apple developer certificate, or create one named \"Leon Local\" (README)."
        );
    } else {
        println!("bundle: signed with \"{identity}\"");
    }
    if !run(
        "codesign",
        &[
            "--force",
            "--deep",
            "--sign",
            &identity,
            &app.to_string_lossy(),
        ],
    ) {
        return false;
    }
    if install {
        let target = Path::new("/Applications/Leon.app");
        if target.exists() && std::fs::remove_dir_all(target).is_err() {
            eprintln!("bundle: could not replace {}", target.display());
            return false;
        }
        // Moved, not copied: a second Leon.app left in target/ would show in
        // Spotlight and Launchpad next to the installed one.
        if !run("mv", &[&app.to_string_lossy(), "/Applications/"]) {
            return false;
        }
        println!("bundle: installed {}", target.display());
    }
    true
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Leon.app/Contents/{Info.plist, MacOS/leon, Resources/leon.icns}.
fn assemble(root: &Path, app: &Path) -> std::io::Result<()> {
    if app.exists() {
        std::fs::remove_dir_all(app)?;
    }
    let contents = app.join("Contents");
    std::fs::create_dir_all(contents.join("MacOS"))?;
    std::fs::create_dir_all(contents.join("Resources"))?;
    std::fs::copy(
        root.join("target/release/leon"),
        contents.join("MacOS/leon"),
    )?;
    std::fs::copy(
        root.join("assets/leon.icns"),
        contents.join("Resources/leon.icns"),
    )?;
    std::fs::write(
        contents.join("Info.plist"),
        PLIST.replace("VERSION", env!("CARGO_PKG_VERSION")),
    )
}

fn run(program: &str, args: &[&str]) -> bool {
    match Command::new(program).args(args).status() {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!("bundle: {program} failed ({status})");
            false
        }
        Err(error) => {
            eprintln!("bundle: could not run {program}: {error}");
            false
        }
    }
}

/// The certificate to sign with: `LEON_SIGN_IDENTITY`, else the best
/// code-signing identity of the keychain (see [`pick_identity`]), else `-`
/// (ad hoc).
fn signing_identity() -> String {
    if let Ok(name) = std::env::var("LEON_SIGN_IDENTITY")
        && !name.trim().is_empty()
    {
        return name;
    }
    let listed = Command::new("security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .map_or_default(|out| String::from_utf8_lossy(&out.stdout).into_owned());
    pick_identity(&listed).unwrap_or_else(|| "-".to_owned())
}

/// From `security find-identity` output (`  1) <hash> "<name>"` lines), the
/// identity macOS keeps folder permissions for across rebuilds: an Apple
/// Developer ID, else an Apple Development certificate (both from an Apple
/// developer account), else the self-signed "Leon Local".
fn pick_identity(listed: &str) -> Option<String> {
    let names: Vec<&str> = listed
        .lines()
        .filter_map(|line| line.split('"').nth(1))
        .collect();
    [
        "Developer ID Application:",
        "Apple Development:",
        "Leon Local",
    ]
    .into_iter()
    .find_map(|kind| names.iter().find(|name| name.starts_with(kind)))
    .map(|name| (*name).to_owned())
}

#[cfg(test)]
mod tests {
    use super::pick_identity;

    #[test]
    fn a_developer_account_certificate_comes_first() {
        let line = |n: u8, name: &str| format!("  {n}) 0123ABCD \"{name}\"\n");
        let dev = "Apple Development: Dev Name (TEAM123)";
        let id = "Developer ID Application: Dev Name (TEAM123)";
        let cases = [
            (
                format!("{}{}", line(1, "Leon Local"), line(2, dev)),
                Some(dev),
            ),
            (format!("{}{}", line(1, dev), line(2, id)), Some(id)),
            (line(1, "Leon Local"), Some("Leon Local")),
            ("     0 valid identities found\n".to_owned(), None),
        ];
        for (listed, expected) in cases {
            assert_eq!(pick_identity(&listed).as_deref(), expected, "{listed}");
        }
    }
}
