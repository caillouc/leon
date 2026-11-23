//! `cargo xtask release`: Leon.app signed with a Developer ID, in a `.dmg`
//! notarized by Apple and stapled, ready to download from a GitHub release
//! (`.github/workflows/release.yml`).
//!
//! Apple requires a Developer ID certificate and notarization for an app
//! downloaded outside the App Store ("Notarizing macOS software before
//! distribution", developer.apple.com, read 2026-11-19). Notarization uses
//! an App Store Connect API key: `LEON_NOTARY_KEY` (the `.p8` file's path),
//! `LEON_NOTARY_KEY_ID` and `LEON_NOTARY_ISSUER`. Without them the `.dmg` is
//! signed but not notarized, and the task says so and fails.

use std::path::Path;
use std::process::Command;

use crate::bundle::{build_app, run, signing_identity, workspace_root};

pub(crate) fn release() -> bool {
    let identity = signing_identity();
    if !identity.name.starts_with("Developer ID Application:") {
        eprintln!(
            "release: needs a \"Developer ID Application\" certificate in the keychain \
             (or LEON_SIGN_IDENTITY); found \"{}\"",
            identity.name
        );
        return false;
    }
    let Some(app) = build_app() else {
        return false;
    };
    let app = app.to_string_lossy().into_owned();
    // The hardened runtime and a secure timestamp: notarization needs both.
    let sign = |path: &str| {
        run(
            "codesign",
            &[
                "--force",
                "--options",
                "runtime",
                "--timestamp",
                "--sign",
                &identity.key,
                path,
            ],
        )
    };
    if !sign(&app) {
        return false;
    }
    println!("release: signed with \"{}\"", identity.name);
    let root = workspace_root();
    let dmg = root.join(format!(
        "target/release/Leon-{}.dmg",
        env!("CARGO_PKG_VERSION")
    ));
    let dmg = dmg.to_string_lossy().into_owned();
    let made = make_dmg(&root, &app, &dmg);
    // Copies of Leon.app left in target/ would show in Spotlight and
    // Launchpad next to the installed one.
    for leftover in [
        root.join("target/release/dmg"),
        root.join("target/release/Leon.app"),
    ] {
        if let Err(error) = std::fs::remove_dir_all(&leftover) {
            eprintln!("release: could not remove {}: {error}", leftover.display());
        }
    }
    if !made || !sign(&dmg) {
        return false;
    }
    println!("release: {dmg}");
    let key = [
        "LEON_NOTARY_KEY",
        "LEON_NOTARY_KEY_ID",
        "LEON_NOTARY_ISSUER",
    ]
    .map(|name| std::env::var(name).ok().filter(|v| !v.trim().is_empty()));
    let [Some(key), Some(key_id), Some(issuer)] = key else {
        eprintln!(
            "release: not notarized: set LEON_NOTARY_KEY (the .p8 file), \
             LEON_NOTARY_KEY_ID and LEON_NOTARY_ISSUER"
        );
        return false;
    };
    notarize(&dmg, &key, &key_id, &issuer) && run("xcrun", &["stapler", "staple", &dmg])
}

/// A `.dmg` holding Leon.app and a link to /Applications to drag it onto.
fn make_dmg(root: &Path, app: &str, dmg: &str) -> bool {
    let stage = root.join("target/release/dmg");
    let staged = std::fs::remove_dir_all(&stage)
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(e)
            }
        })
        .and_then(|()| std::fs::create_dir_all(&stage))
        .and_then(|()| std::os::unix::fs::symlink("/Applications", stage.join("Applications")));
    if let Err(error) = staged {
        eprintln!("release: {error}");
        return false;
    }
    let stage = stage.to_string_lossy().into_owned();
    run("ditto", &[app, &format!("{stage}/Leon.app")])
        && run(
            "hdiutil",
            &[
                "create",
                "-volname",
                "Leon",
                "-srcfolder",
                &stage,
                "-ov",
                "-format",
                "UDZO",
                dmg,
            ],
        )
}

/// Sends the `.dmg` to Apple's notary service and waits for its answer:
/// `true` only when Apple accepts it.
fn notarize(dmg: &str, key: &str, key_id: &str, issuer: &str) -> bool {
    println!("release: sent to Apple for notarization, waiting…");
    let output = Command::new("xcrun")
        .args([
            "notarytool",
            "submit",
            dmg,
            "--key",
            key,
            "--key-id",
            key_id,
            "--issuer",
            issuer,
            "--wait",
            "--output-format",
            "json",
        ])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            eprintln!("release: could not run notarytool: {error}");
            return false;
        }
    };
    let answer: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
    let status = answer["status"].as_str().unwrap_or("unknown");
    if status == "Accepted" {
        println!("release: notarized");
        return true;
    }
    let id = answer["id"].as_str().unwrap_or("?");
    eprintln!(
        "release: Apple answered \"{status}\". Its log: xcrun notarytool log {id} --key … \
         --key-id … --issuer …\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    false
}
