//! Codex's plan without a turn (DOCTOR-2): a short `codex app-server` run —
//! `initialize`, `initialized`, then `account/rateLimits/read`, whose
//! `rateLimits` has the shape of the `account/rateLimits/updated`
//! notification (app-server 0.160.0, a real run, 2026-11-24). Claude Code
//! and Copilot CLI tell theirs only during a turn, or not at all.

use std::ffi::OsString;

use leon_core::ports::PlanSaid;
use serde_json::json;

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
