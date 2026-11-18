//! Opt-in contract test (AGENTS.md): the reads against GitHub with this
//! machine's `gh` login; never writes, prints only counts.
//! `cargo test -p leon-github --test live -- --ignored --nocapture`

#![allow(clippy::print_stderr, reason = "the report is the point")]

use leon_core::ports::GitHub;
use leon_domain::Issue;
use leon_github::GhCli;

#[test]
#[ignore = "reads GitHub with the user's gh login"]
fn reads_by_number() {
    let gh = GhCli::default();
    let pr = gh.pull_request_by_number("cli/cli", 15083).unwrap();
    assert_eq!((pr.repo.as_str(), pr.number), ("cli/cli", 15083));
    assert!(!pr.checks.is_empty() && pr.rollup.is_some());
    let issue = gh.issue("cli/cli", 14962).unwrap();
    assert_eq!((issue.repo.as_str(), issue.number), ("cli/cli", 14962));
    assert_ne!(issue.body, "");
    let missing = gh
        .pull_request_by_number("cli/cli", 99_999_999)
        .unwrap_err();
    assert!(
        missing.message.starts_with("cli/cli#99999999: "),
        "{missing:?}"
    );
    assert!(gh.issue("cli", 1).is_err());
}

/// cli/cli#15081 had unresolved threads on lines, cli/cli#15007 only
/// outdated ones without a line (2026-11-18).
#[test]
#[ignore = "reads GitHub with the user's gh login"]
fn reads_review_threads() {
    let gh = GhCli::default();
    for number in [15081, 15007] {
        let pr = gh.pull_request_by_number("cli/cli", number).unwrap();
        assert!(
            pr.threads
                .iter()
                .all(|t| !t.path.is_empty() && !t.comments.is_empty())
        );
        eprintln!(
            "cli/cli#{number}: body {} chars, {} unresolved threads ({} without a line), {} comments in them, {} code lines, author {:?}",
            pr.body.len(),
            pr.threads.len(),
            pr.threads.iter().filter(|t| t.line.is_none()).count(),
            pr.threads.iter().map(|t| t.comments.len()).sum::<usize>(),
            pr.threads.iter().map(|t| t.code.len()).sum::<usize>(),
            pr.author,
        );
    }
}

#[test]
#[ignore = "reads the user's own issues with their gh login"]
fn my_issues_with_their_board_fields() {
    let gh = GhCli::default();
    let issues = gh.my_issues().unwrap();
    let count = |has: fn(&Issue) -> bool| issues.iter().filter(|i| has(i)).count();
    eprintln!(
        "{} issues: {} with a status, {} with a priority, {} with an iteration",
        issues.len(),
        count(|i| i.status.is_some()),
        count(|i| i.priority.is_some()),
        count(|i| !i.iterations.is_empty()),
    );
    for listed in issues.iter().take(3) {
        let alone = gh.issue(&listed.repo, listed.number).unwrap();
        assert_eq!(
            (&alone.status, &alone.priority, &alone.iterations),
            (&listed.status, &listed.priority, &listed.iterations)
        );
        eprintln!(
            "one issue: {} assignees, {} comments, body {}",
            alone.assignees.len(),
            alone.comments.len(),
            if alone.body.is_empty() {
                "empty"
            } else {
                "set"
            },
        );
    }
}
