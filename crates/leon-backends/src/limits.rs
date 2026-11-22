//! A CLI's plan limits, from what it reports (DOCTOR-2). Claude Code sends a
//! `rate_limit_event` on stream-json (2.1.288, run 2026-11-20); Codex an
//! `account/rateLimits/updated` notification (app-server 0.160.0,
//! recorded). Copilot CLI sends none that Leon knows of.

use leon_domain::LimitWindow;
use serde_json::Value;

/// Claude Code's windows, from its `rate_limit_info`: the 5-hour and the
/// weekly one; without them (an older CLI), the one that limits now.
pub(crate) fn claude(info: &Value) -> Option<Vec<LimitWindow>> {
    let windows: Vec<LimitWindow> = [("five_hour", 300), ("seven_day", 10_080)]
        .into_iter()
        .filter_map(|(key, minutes)| {
            let window = &info["unifiedWindows"][key];
            window_of(
                minutes,
                window["utilization"].as_f64()? * 100.,
                &window["resetsAt"],
            )
        })
        .collect();
    if !windows.is_empty() {
        return Some(windows);
    }
    let minutes = match info["rateLimitType"].as_str()? {
        "five_hour" => 300,
        "seven_day" | "seven_day_opus" | "seven_day_sonnet" => 10_080,
        _ => return None,
    };
    window_of(
        minutes,
        info["utilization"].as_f64()? * 100.,
        &info["resetsAt"],
    )
    .map(|w| vec![w])
}

/// Codex's windows, from its `rateLimits`: the primary and the secondary
/// one, shortest first.
pub(crate) fn codex(limits: &Value) -> Option<Vec<LimitWindow>> {
    let mut windows: Vec<LimitWindow> = ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| {
            let window = &limits[key];
            let minutes = u32::try_from(window["windowDurationMins"].as_u64()?).ok()?;
            window_of(
                minutes,
                window["usedPercent"].as_f64()?,
                &window["resetsAt"],
            )
        })
        .collect();
    windows.sort_by_key(|w| w.minutes);
    (!windows.is_empty()).then_some(windows)
}

/// A window `percent` used, resetting at `resets_at` (Unix seconds).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "rounded and kept within 0..=10 000 first"
)]
fn window_of(minutes: u32, percent: f64, resets_at: &Value) -> Option<LimitWindow> {
    percent.is_finite().then(|| LimitWindow {
        minutes,
        used: percent.round().clamp(0., 10_000.) as u32,
        resets_at: resets_at.as_i64().map(|s| s * 1000),
    })
}

#[cfg(test)]
mod tests {
    use leon_domain::LimitWindow;
    use serde_json::{Value, json};

    use super::{claude, codex};

    fn window(minutes: u32, used: u32, resets_at: i64) -> LimitWindow {
        LimitWindow {
            minutes,
            used,
            resets_at: Some(resets_at * 1000),
        }
    }

    #[test]
    fn claude_code_reports_both_windows() {
        let line = include_str!("../fixtures/claude/2.1.288/rate-limit.jsonl");
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(
            claude(&v["rate_limit_info"]),
            Some(vec![
                window(300, 3, 1_795_180_200),
                window(10_080, 83, 1_795_374_000),
            ])
        );
        // Without its windows: the one that limits now.
        let older = json!({"status": "allowed", "rateLimitType": "five_hour", "utilization": 0.5, "resetsAt": 10});
        assert_eq!(claude(&older), Some(vec![window(300, 50, 10)]));
        assert_eq!(claude(&json!({"status": "allowed"})), None);
    }

    #[test]
    fn codex_reports_its_windows_shortest_first() {
        let limits = json!({
            "primary": {"usedPercent": 40, "windowDurationMins": 10080, "resetsAt": 20},
            "secondary": {"usedPercent": 12.6, "windowDurationMins": 300, "resetsAt": 10},
        });
        assert_eq!(
            codex(&limits),
            Some(vec![window(300, 13, 10), window(10_080, 40, 20)])
        );
        assert_eq!(codex(&json!({"primary": null, "secondary": null})), None);
    }
}
