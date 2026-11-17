//! `cargo xtask bundle [--install]`: builds Leon.app (macOS) from the release
//! binary, the icon in `assets/` and an Info.plist, signs it ad hoc so macOS
//! runs it, and with `--install` copies it to /Applications.

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
    if !run(
        "codesign",
        &["--force", "--deep", "--sign", "-", &app.to_string_lossy()],
    ) {
        return false;
    }
    if install {
        let target = Path::new("/Applications/Leon.app");
        if target.exists() && std::fs::remove_dir_all(target).is_err() {
            eprintln!("bundle: could not replace {}", target.display());
            return false;
        }
        if !run("cp", &["-R", &app.to_string_lossy(), "/Applications/"]) {
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
