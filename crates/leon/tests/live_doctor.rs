//! Opt-in contract test (AGENTS.md): the doctor's checks against the real
//! tools on this machine. `cargo test -p leon --test live_doctor -- --ignored --nocapture`

#![allow(clippy::print_stderr, reason = "the report is the point")]

use leon_core::ports::{Backends, Git, GitHub};

#[test]
#[ignore = "runs the real CLIs, git and gh"]
fn doctor_reports_every_tool() {
    let mut sections = leon_backends::Clis::default().doctor();
    sections.push(leon_git::GitCli::default().doctor());
    sections.push(leon_github::GhCli::default().doctor());
    for section in &sections {
        eprintln!("{} ({:?})", section.title, section.health());
        for c in &section.checks {
            eprintln!(
                "  {:?} {}: {} {}",
                c.health,
                c.name,
                c.detail,
                c.fix.as_deref().unwrap_or("")
            );
        }
    }
    let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Claude Code", "Codex", "Copilot CLI", "git", "GitHub CLI"]
    );
}
