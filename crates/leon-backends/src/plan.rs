//! A CLI's plan without a turn (DOCTOR-2). Claude Code: a `get_usage`
//! control request on stream-json (2.1.332, a real run, 2026-11-27). Codex:
//! a short `codex app-server` run — `initialize`, `initialized`, then
//! `account/rateLimits/read`, whose `rateLimits` has the shape of the
//! `account/rateLimits/updated` notification (app-server 0.160.0, a real
//! run, 2026-11-24). Copilot CLI tells none.

use std::ffi::OsString;

use leon_core::ports::PlanSaid;
use serde_json::json;

/// What Claude Code says of its plan now; `None`, logged, when it could
/// not tell (an API key has no plan).
pub(crate) fn claude(path: Option<&OsString>) -> Option<PlanSaid> {
    let answer = crate::ask::claude_control(path, "get_usage")?;
    crate::limits::claude_usage(&answer).map(|windows| (windows, None))
}

/// What Codex says of its plan now; `None`, logged, when it could not tell.
pub(crate) fn codex(path: Option<&OsString>) -> Option<PlanSaid> {
    crate::ask::ask(
        path,
        "codex",
        &["app-server"],
        &crate::models::codex_initialize(),
        |v, send| match v["id"].as_i64() {
            Some(1) => {
                let sent = send(json!({ "jsonrpc": "2.0", "method": "initialized" }))
                    && send(
                        json!({ "jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read" }),
                    );
                // Could not ask: no answer.
                (!sent).then_some(None)
            }
            Some(2) => Some(crate::limits::codex_said(&v["result"]["rateLimits"])),
            _ => None,
        },
    )
    .flatten()
}
