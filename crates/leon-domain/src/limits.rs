//! How much of a CLI's plan the user has used (DOCTOR-2), as the CLI says
//! it: the share of each window its limit counts in (5 hours, a week…) and
//! when that window starts again.

/// One window of a plan's limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitWindow {
    /// Its length, in minutes: 300 for 5 hours, 10 080 for a week.
    pub minutes: u32,
    /// How much of it is used, in percent; above 100 past the cap.
    pub used: u32,
    /// When it starts again, Unix milliseconds.
    pub resets_at: Option<i64>,
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
    use super::window_name;

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
