//! Developer tasks.
//!
//! - `cargo xtask ci`: every check CI runs (fmt, clippy, tests, layers,
//!   cargo-deny).
//! - `cargo xtask layers`: checks that crate dependencies follow
//!   `spec/architecture.md`.
//! - `cargo xtask bundle [--install]`: builds Leon.app (macOS), and copies
//!   it to /Applications with `--install`.
//! - `cargo xtask release`: Leon.app signed with a Developer ID, in a
//!   notarized `.dmg` to publish (macOS).

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool reports on the terminal"
)]

mod bundle;
mod layers;
mod release;

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let ok = match std::env::args().nth(1).as_deref() {
        Some("ci") => ci(),
        Some("layers") => layers::check(),
        Some("bundle") => bundle::bundle(std::env::args().any(|a| a == "--install")),
        Some("release") => release::release(),
        _ => {
            eprintln!("usage: cargo xtask <ci|layers|bundle [--install]|release>");
            false
        }
    };
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn ci() -> bool {
    let ok = cargo(&["fmt", "--all", "--check"])
        && cargo(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ])
        // The offscreen screenshot tool sits behind a feature.
        && cargo(&[
            "clippy",
            "-p",
            "leon-ui",
            "--all-targets",
            "--features",
            "preview",
            "--locked",
            "--",
            "-D",
            "warnings",
        ])
        && cargo(&["test", "--workspace", "--locked"])
        && layers::check()
        && deny();
    if ok {
        println!("ci: all checks passed");
    }
    ok
}

fn deny() -> bool {
    let ok = cargo(&["deny", "--log-level", "error", "check"]);
    if !ok {
        eprintln!("cargo-deny failed or is missing (install: cargo install --locked cargo-deny)");
    }
    ok
}

/// Runs `cargo <args>` in the workspace and says whether it succeeded.
fn cargo(args: &[&str]) -> bool {
    println!("$ cargo {}", args.join(" "));
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    match Command::new(cargo).args(args).status() {
        Ok(status) => status.success(),
        Err(error) => {
            eprintln!("could not run cargo: {error}");
            false
        }
    }
}
