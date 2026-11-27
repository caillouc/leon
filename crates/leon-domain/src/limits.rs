//! How much of a CLI's plan the user has used (DOCTOR-2), as the CLI says
//! it: the share of each window its limit counts in (5 hours, a week…) and
//! when that window starts again.

use serde::{Deserialize, Serialize};

use crate::BackendKind;

/// What a CLI last said of its plan (DOCTOR-2, USE-1): its windows, the
/// credits its account has left, and when it said it, in Unix milliseconds.
/// Kept across restarts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanUsage {
    /// The CLI.
    pub backend: BackendKind,
    /// Its windows, shortest first.
    pub windows: Vec<LimitWindow>,
    /// The credits its account has left, when it has some to count.
    pub credits: Option<String>,
    /// When it said it.
    pub at: i64,
}

/// One window of a plan's limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitWindow {
    /// Its length, in minutes: 300 for 5 hours, 10 080 for a week.
    pub minutes: u32,
    /// How much of it is used, in percent; above 100 past the cap.
    pub used: u32,
    /// When it starts again, Unix milliseconds.
    pub resets_at: Option<i64>,
    /// The model it alone counts, when not every one: Claude Code's Fable
    /// limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl LimitWindow {
    /// Its name: its length, after the model it alone counts ("Fable
    /// week").
    pub fn name(&self) -> String {
        let length = window_name(self.minutes);
        self.model
            .as_ref()
            .map_or(length.clone(), |m| format!("{m} {length}"))
    }
}

/// `said`, what a CLI says of its windows now, and the windows of one
/// model it said `before` that it no longer mentions, until they reset: a
/// CLI tells a model's own window only on that model's turns (Claude
/// Code's Fable limit).
pub fn with_model_windows(
    said: Vec<LimitWindow>,
    before: &[LimitWindow],
    now: i64,
) -> Vec<LimitWindow> {
    let kept: Vec<LimitWindow> = before
        .iter()
        .filter(|w| w.model.is_some() && w.resets_at.is_none_or(|at| at > now))
        .filter(|w| {
            !said
                .iter()
                .any(|s| s.model == w.model && s.minutes == w.minutes)
        })
        .cloned()
        .collect();
    said.into_iter().chain(kept).collect()
}

/// A window's name, from its length, in the app's short units: "5h",
/// "week", "30d".
pub fn window_name(minutes: u32) -> String {
    match minutes {
        10_080 => "week".to_owned(),
        m if m % 1_440 == 0 => format!("{}d", m / 1_440),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::{LimitWindow, window_name, with_model_windows};

    /// A model's window stays when a later report leaves it out, until it
    /// resets; a newer one replaces it.
    #[test]
    fn a_model_s_window_stays_until_it_resets() {
        let window = |used, model: Option<&str>, resets_at| LimitWindow {
            minutes: 10_080,
            used,
            resets_at: Some(resets_at),
            model: model.map(str::to_owned),
        };
        let fable = window(11, Some("Fable"), 1_000);
        let week = window(70, None, 1_000);
        let kept = with_model_windows(vec![week.clone()], &[week.clone(), fable.clone()], 500);
        assert_eq!(kept, [week.clone(), fable.clone()]);
        // Reset since: gone.
        assert_eq!(
            with_model_windows(vec![week.clone()], std::slice::from_ref(&fable), 2_000),
            std::slice::from_ref(&week)
        );
        // Said again: the new one.
        let newer = window(15, Some("Fable"), 1_000);
        assert_eq!(
            with_model_windows(vec![week.clone(), newer.clone()], &[fable], 500),
            [week, newer]
        );
    }

    #[test]
    fn windows_are_named_by_their_length() {
        let cases = [
            (300, "5h"),
            (10_080, "week"),
            (43_200, "30d"),
            (1_440, "1d"),
            (90, "90m"),
        ];
        for (minutes, name) in cases {
            assert_eq!(window_name(minutes), name, "{minutes}");
        }
    }
}
