//! A short exchange with a CLI, without a turn: JSON lines written to it,
//! its JSON lines read until the answer comes, then it is stopped. For what
//! a CLI tells only when asked: each CLI's plan (DOCTOR-2) and models
//! (I31).

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// How long a CLI may take to answer.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Runs `program` with `args` in the temporary folder, writes `first`, then
/// gives each line it prints to `on_line` with a way to write more, until
/// `on_line` has the answer. `None`, logged, when the CLI could not start,
/// ended, or took too long.
pub(crate) fn ask<T>(
    path: Option<&OsString>,
    program: &str,
    args: &[&str],
    first: &Value,
    mut on_line: impl FnMut(&Value, &mut dyn FnMut(Value) -> bool) -> Option<T>,
) -> Option<T> {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let mut child = crate::process::command(path, program, &args, &std::env::temp_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| tracing::debug!(%error, program, "asking a CLI"))
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
    let answer = (|| {
        send(first.clone()).then_some(())?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            let line = rx.recv_timeout(left).ok()?;
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && let Some(answer) = on_line(&value, &mut send)
            {
                return Some(answer);
            }
        }
    })();
    if answer.is_none() {
        tracing::debug!(program, "a CLI did not answer");
    }
    if let Err(error) = child.kill() {
        tracing::debug!(%error, program, "stopping a CLI");
    }
    let _ = child.wait();
    answer
}

/// Claude Code's answer to a control request of `subtype` on stream-json,
/// which runs no turn and leaves no session behind (2.1.332, real runs,
/// 2026-11-26 and 27): its `response`.
pub(crate) fn claude_control(path: Option<&OsString>, subtype: &str) -> Option<Value> {
    let request = json!({ "type": "control_request", "request_id": "leon",
        "request": { "subtype": subtype } });
    let args = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        // Nothing left behind to resume.
        "--no-session-persistence",
    ];
    ask(path, "claude", &args, &request, |v, _| {
        (v["type"] == "control_response" && v["response"]["request_id"] == "leon")
            .then(|| v["response"]["response"].clone())
    })
}
