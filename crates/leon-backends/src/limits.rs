//! A CLI's plan limits, from what it reports (DOCTOR-2). Claude Code sends a
//! `rate_limit_event` on stream-json (2.1.288, run 2026-11-20) and answers
//! `get_usage` (2.1.332); Codex an
//! `account/rateLimits/updated` notification (app-server 0.160.0,
//! recorded). Copilot CLI sends none that Leon knows of.

use leon_domain::LimitWindow;
use serde_json::Value;

/// Claude Code's windows, from its `rate_limit_info`: the 5-hour and the
/// weekly one, and the weekly one of Fable when the account has one
/// (`seven_day_overage_included`, "Fable limit" in Claude Code 2.1.332's
/// own labels, sent once Fable is used: a real run, 2026-11-27); without
/// them (an older CLI), the one that limits now.
pub(crate) fn claude(info: &Value) -> Option<Vec<LimitWindow>> {
    let keys = [
        ("five_hour", 300, None),
        ("seven_day", 10_080, None),
        ("seven_day_overage_included", 10_080, Some("Fable")),
    ];
    let windows: Vec<LimitWindow> = keys
        .into_iter()
        .filter_map(|(key, minutes, model)| {
            let window = &info["unifiedWindows"][key];
            let found = window_of(
                minutes,
                window["utilization"].as_f64()? * 100.,
                &window["resetsAt"],
            )?;
            Some(LimitWindow {
                model: model.map(str::to_owned),
                ..found
            })
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

/// Claude Code's windows, from its answer to a `get_usage` control
/// request (2.1.332, a real run, 2026-11-27): each of `rate_limits.limits`,
/// its `group` giving its length (`session`, the 5-hour one: same use and
/// reset as `five_hour` in that run; `weekly`), its `scope` the model it
/// counts, `resets_at` an RFC 3339 date.
pub(crate) fn claude_usage(usage: &Value) -> Option<Vec<LimitWindow>> {
    let windows: Vec<LimitWindow> = usage["rate_limits"]["limits"]
        .as_array()?
        .iter()
        .filter_map(|limit| {
            let minutes = match limit["group"].as_str()? {
                "session" => 300,
                "weekly" => 10_080,
                _ => return None,
            };
            Some(LimitWindow {
                resets_at: limit["resets_at"].as_str().and_then(leon_domain::unix_ms),
                model: limit["scope"]["model"]["display_name"]
                    .as_str()
                    .map(str::to_owned),
                ..window_of(minutes, limit["percent"].as_f64()?, &Value::Null)?
            })
        })
        .collect();
    (!windows.is_empty()).then_some(windows)
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

/// What Codex's `rateLimits` say: its windows and its credits; `None` when
/// they say neither.
pub(crate) fn codex_said(limits: &Value) -> Option<leon_core::ports::PlanSaid> {
    let windows = codex(limits).unwrap_or_default();
    let credits = codex_credits(limits);
    (!windows.is_empty() || credits.is_some()).then_some((windows, credits))
}

/// Codex's credits balance, from its `rateLimits.credits` (`balance`,
/// `hasCredits`, `unlimited`; app-server schema of 0.160.0, 2026-11-24):
/// shown only when the account has credits and they are not unlimited
/// (USE-1).
fn codex_credits(limits: &Value) -> Option<String> {
    let credits = &limits["credits"];
    if credits["hasCredits"] != true || credits["unlimited"] == true {
        return None;
    }
    credits["balance"].as_str().map(str::to_owned)
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
        model: None,
    })
}

#[cfg(test)]
mod tests {
    use leon_domain::LimitWindow;
    use serde_json::{Value, json};

    use super::{claude, claude_usage, codex, codex_credits};

    #[test]
    fn codex_credits_show_only_when_counted() {
        let cases = [
            (
                json!({ "credits": { "hasCredits": true, "unlimited": false, "balance": "1250" } }),
                Some("1250"),
            ),
            (
                json!({ "credits": { "hasCredits": true, "unlimited": true, "balance": "0" } }),
                None,
            ),
            (
                json!({ "credits": { "hasCredits": false, "unlimited": false, "balance": null } }),
                None,
            ),
            (json!({ "primary": {} }), None),
        ];
        for (limits, expected) in cases {
            assert_eq!(codex_credits(&limits).as_deref(), expected, "{limits}");
        }
    }

    fn window(minutes: u32, used: u32, resets_at: i64) -> LimitWindow {
        LimitWindow {
            minutes,
            used,
            resets_at: Some(resets_at * 1000),
            model: None,
        }
    }

    /// A Fable turn also reports Fable's own weekly window.
    #[test]
    fn claude_code_reports_fable_s_window() {
        let line = include_str!("../fixtures/claude/2.1.332/rate-limit-fable.jsonl");
        let event: Value = serde_json::from_str(line.trim()).unwrap();
        let windows = claude(&event["rate_limit_info"]).unwrap();
        let named: Vec<(String, u32)> = windows.iter().map(|w| (w.name(), w.used)).collect();
        assert_eq!(
            named,
            [
                ("5h".to_owned(), 83),
                ("week".to_owned(), 70),
                ("Fable week".to_owned(), 11)
            ]
        );
    }

    /// Asked, without a turn: every window, Fable's included.
    #[test]
    fn claude_code_tells_its_usage_when_asked() {
        let line = include_str!("../fixtures/claude/2.1.332/get-usage.jsonl");
        let answer: Value = serde_json::from_str(line.trim()).unwrap();
        let windows = claude_usage(&answer["response"]["response"]).unwrap();
        let fable_week = LimitWindow {
            model: Some("Fable".into()),
            ..window(10_080, 12, 1_795_978_799)
        };
        assert_eq!(
            windows,
            [
                window(300, 87, 1_795_803_599),
                window(10_080, 71, 1_795_978_799),
                fable_week,
            ]
        );
        let none = json!({ "rate_limits": { "limits": [{ "group": "monthly", "percent": 3 }] } });
        assert_eq!(claude_usage(&none), None);
        assert_eq!(claude_usage(&json!({ "rate_limits": null })), None);
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
