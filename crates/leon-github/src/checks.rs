//! CI checks of a pull request: the `statusCheckRollup` of its last commit.

use leon_domain::{Check, CheckState};
use serde_json::Value;

use crate::parse::{nodes, text};

/// Checks of a pull request node; empty when the query did not ask for them.
pub(crate) fn checks(pr: &Value) -> Vec<Check> {
    let rollup = &pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"];
    nodes(&rollup["contexts"]).filter_map(check).collect()
}

/// A GitHub Actions or app check (`CheckRun`), or a commit status
/// (`StatusContext`); `None` for anything else.
fn check(context: &Value) -> Option<Check> {
    let (name, state) = match context["__typename"].as_str()? {
        "CheckRun" => (
            &context["name"],
            check_run_state(context["status"].as_str(), context["conclusion"].as_str()),
        ),
        "StatusContext" => (&context["context"], status_state(context["state"].as_str())),
        _ => return None,
    };
    Some(Check {
        name: text(name)?,
        state,
    })
}

/// Pending until completed; then neutral and skipped pass, and a stale run
/// (never finished, closed by GitHub) fails like a timed-out one.
fn check_run_state(status: Option<&str>, conclusion: Option<&str>) -> CheckState {
    if status != Some("COMPLETED") {
        return CheckState::Pending;
    }
    match conclusion {
        Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => CheckState::Pass,
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
        use CheckState::{Fail, Pass, Pending};
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
            (run("COMPLETED", Some("SKIPPED")), Pass),
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
                    state: expected
                }],
                "{context}"
            );
        }
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
