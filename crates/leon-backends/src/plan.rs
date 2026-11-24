//! Codex's plan without a turn (DOCTOR-2): a short `codex app-server` run —
//! `initialize`, `initialized`, then `account/rateLimits/read`, whose
//! `rateLimits` has the shape of the `account/rateLimits/updated`
//! notification (app-server 0.160.0, a real run, 2026-11-24). Claude Code
//! and Copilot CLI tell theirs only during a turn, or not at all.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::time::Duration;

use leon_core::ports::PlanSaid;
use serde_json::{Value, json};

/// How long Codex may take to answer.
const TIMEOUT: Duration = Duration::from_secs(20);

/// What Codex says of its plan now; `None`, logged, when it could not tell.
pub(crate) fn codex(path: Option<&OsString>) -> Option<PlanSaid> {
    let mut child = crate::process::command(
        path,
        "codex",
        &["app-server".to_owned()],
        &std::env::temp_dir(),
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|error| tracing::debug!(%error, "codex app-server for its plan"))
    .ok()?;
    let mut input = child.stdin.take()?;
    let output = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    let mut send = |message: Value| writeln!(input, "{message}").is_ok();
    let said = (|| {
        send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "clientInfo": { "name": "leon", "version": env!("CARGO_PKG_VERSION") } } }))
        .then_some(())?;
        let deadline = std::time::Instant::now() + TIMEOUT;
        loop {
            let left = deadline.checked_duration_since(std::time::Instant::now())?;
            let line = rx.recv_timeout(left).ok()?;
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            match v["id"].as_i64() {
                Some(1) => {
                    send(json!({ "jsonrpc": "2.0", "method": "initialized" })).then_some(())?;
                    send(json!({ "jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read" }))
                        .then_some(())?;
                }
                Some(2) => return crate::limits::codex_said(&v["result"]["rateLimits"]),
                _ => {}
            }
        }
    })();
    if let Err(error) = child.kill() {
        tracing::debug!(%error, "stopping codex app-server");
    }
    let _ = child.wait();
    said
}
