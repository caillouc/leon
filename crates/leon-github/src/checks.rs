//! CI checks of a pull request: the `statusCheckRollup` of its last commit.

use leon_domain::{Check, CheckState};
use serde_json::Value;

use crate::parse::{nodes, text};

/// Checks of a pull request node; empty when the query did not ask for them.
/// A job run more than once counts once, its latest run, as GitHub shows
/// them: the most recently started per name, workflow and event, a status
/// once per context (`eliminateDuplicates` in cli/cli's
/// `pkg/cmd/pr/checks/aggregate.go`, read on 2026-11-24).
pub(crate) fn checks(pr: &Value) -> Vec<Check> {
    let rollup = &pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"];
    let mut found: Vec<&Value> = nodes(&rollup["contexts"]).collect();
    // Latest first; one never started (`null`) counts as the oldest.
    found.sort_by(|a, b| b["startedAt"].as_str().cmp(&a["startedAt"].as_str()));
    let mut seen = std::collections::HashSet::new();
    found
        .into_iter()
        .filter(|c| seen.insert(key(c)))
        .filter_map(check)
        .collect()
}

/// What makes two contexts the same check: a status's context; a run's
/// name, workflow and event.
fn key(context: &Value) -> String {
    if let Some(status) = context["context"].as_str() {
        return format!("status/{status}");
    }
    let run = &context["checkSuite"]["workflowRun"];
    format!(
        "run/{}/{}/{}",
        context["name"].as_str().unwrap_or_default(),
        run["workflow"]["name"].as_str().unwrap_or_default(),
        run["event"].as_str().unwrap_or_default()
    )
}

/// A GitHub Actions or app check (`CheckRun`), or a commit status
/// (`StatusContext`); `None` for anything else.
fn check(context: &Value) -> Option<Check> {
    let (name, state, url) = match context["__typename"].as_str()? {
        "CheckRun" => (
            &context["name"],
            check_run_state(context["status"].as_str(), context["conclusion"].as_str()),
            &context["detailsUrl"],
        ),
        "StatusContext" => (
            &context["context"],
            status_state(context["state"].as_str()),
            &context["targetUrl"],
        ),
        _ => return None,
    };
    Some(Check {
        name: text(name)?,
        state,
        url: text(url).filter(|u| !u.is_empty()),
    })
}

/// Pending until completed; then neutral passes, skipped is its own, and a
/// stale run
/// (never finished, closed by GitHub) fails like a timed-out one.
fn check_run_state(status: Option<&str>, conclusion: Option<&str>) -> CheckState {
    if status != Some("COMPLETED") {
        return CheckState::Pending;
    }
    match conclusion {
        Some("SUCCESS" | "NEUTRAL") => CheckState::Pass,
        Some("SKIPPED") => CheckState::Skipped,
        Some(
            "FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE" | "STALE",
        ) => CheckState::Fail,
        _ => CheckState::Pending,
    }
}

fn status_state(state: Option<&str>) -> CheckState {
    match state {
        Some("SUCCESS") => CheckState::Pass,
        Some("ERROR" | "FAILURE") => CheckState::Fail,
        _ => CheckState::Pending,
    }
}

#[cfg(test)]
mod tests {
    use leon_domain::{Check, CheckState};
    use serde_json::json;

    use super::checks;

    /// Every value of GitHub's `CheckStatusState`, `CheckConclusionState`
    /// and `StatusState` enums (schema introspection, 2026-11-17).
    #[test]
    fn states() {
        use CheckState::{Fail, Pass, Pending, Skipped};
        let run = |status: &str, conclusion: Option<&str>| json!({ "__typename": "CheckRun", "name": "c", "status": status, "conclusion": conclusion });
        let status =
            |state: &str| json!({ "__typename": "StatusContext", "context": "c", "state": state });
        let cases = [
            (run("REQUESTED", None), Pending),
            (run("QUEUED", None), Pending),
            (run("IN_PROGRESS", None), Pending),
            (run("WAITING", None), Pending),
            (run("PENDING", None), Pending),
            (run("COMPLETED", Some("SUCCESS")), Pass),
            (run("COMPLETED", Some("NEUTRAL")), Pass),
            (run("COMPLETED", Some("SKIPPED")), Skipped),
            (run("COMPLETED", Some("FAILURE")), Fail),
            (run("COMPLETED", Some("CANCELLED")), Fail),
            (run("COMPLETED", Some("TIMED_OUT")), Fail),
            (run("COMPLETED", Some("ACTION_REQUIRED")), Fail),
            (run("COMPLETED", Some("STARTUP_FAILURE")), Fail),
            (run("COMPLETED", Some("STALE")), Fail),
            (run("COMPLETED", Some("SOMETHING_NEW")), Pending),
            (status("SUCCESS"), Pass),
            (status("ERROR"), Fail),
            (status("FAILURE"), Fail),
            (status("PENDING"), Pending),
            (status("EXPECTED"), Pending),
        ];
        for (context, expected) in cases {
            let pr = json!({ "commits": { "nodes": [{ "commit": { "statusCheckRollup": {
                "contexts": { "nodes": [context.clone()] } } } }] } });
            let found = checks(&pr);
            assert_eq!(
                found,
                [Check {
                    name: "c".into(),
                    state: expected,
                    url: None,
                }],
                "{context}"
            );
        }
    }

    /// A job run again — a second workflow run for the same push — counts
    /// once, its latest run: as `gh pr checks` and GitHub show them. Each
    /// workflow and event keeps its own (cli/cli `aggregate.go`).
    #[test]
    fn each_check_counts_once_its_latest_run() {
        let run = |name: &str,
                   status: &str,
                   conclusion: Option<&str>,
                   started: Option<&str>,
                   workflow: &str,
                   event: &str| {
            json!({ "__typename": "CheckRun", "name": name, "status": status,
                "conclusion": conclusion, "startedAt": started,
                "checkSuite": { "workflowRun": { "event": event, "workflow": { "name": workflow } } } })
        };
        let contexts = [
            run(
                "pytest",
                "COMPLETED",
                Some("SUCCESS"),
                Some("2026-11-24T13:53:04Z"),
                "Tests",
                "pull_request",
            ),
            run(
                "pytest",
                "IN_PROGRESS",
                None,
                Some("2026-11-24T13:53:06Z"),
                "Tests",
                "pull_request",
            ),
            run(
                "sync",
                "COMPLETED",
                Some("SUCCESS"),
                Some("2026-11-24T13:52:59Z"),
                "Sync on push",
                "push",
            ),
            run(
                "sync",
                "COMPLETED",
                Some("SUCCESS"),
                Some("2026-11-24T13:53:05Z"),
                "Sync on PR",
                "pull_request",
            ),
            run(
                "lint",
                "COMPLETED",
                Some("FAILURE"),
                Some("2026-11-24T13:53:00Z"),
                "Tests",
                "pull_request",
            ),
            // Queued, not started: older than any started run, as for gh.
            run("lint", "QUEUED", None, None, "Tests", "pull_request"),
        ];
        let pr = json!({ "commits": { "nodes": [{ "commit": { "statusCheckRollup": {
            "contexts": { "nodes": contexts } } } }] } });
        let found: Vec<(String, CheckState)> =
            checks(&pr).into_iter().map(|c| (c.name, c.state)).collect();
        let mut found = found;
        found.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            found,
            [
                ("lint".to_owned(), CheckState::Fail),
                ("pytest".to_owned(), CheckState::Pending),
                ("sync".to_owned(), CheckState::Pass),
                ("sync".to_owned(), CheckState::Pass),
            ]
        );
    }

    #[test]
    fn unknown_or_missing_contexts_are_skipped() {
        let pr = json!({ "commits": { "nodes": [{ "commit": { "statusCheckRollup": {
            "contexts": { "nodes": [{ "__typename": "Other" }, { "__typename": "CheckRun" }] } } } }] } });
        let none: Vec<Check> = Vec::new();
        assert_eq!(checks(&pr), none);
        assert_eq!(checks(&json!({})), none);
        let no_rollup =
            json!({ "commits": { "nodes": [{ "commit": { "statusCheckRollup": null } }] } });
        assert_eq!(checks(&no_rollup), none);
    }
}
