//! Calendar days, as GitHub gives a sprint's dates (`2026-11-09`).

/// A calendar day, counted in days since 1970-01-01.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day(pub i64);

impl Day {
    /// A `YYYY-MM-DD` date; `None` when it is not one.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('-').map(|p| p.parse::<u32>().ok());
        let [year, month, day] = [parts.next()??, parts.next()??, parts.next()??];
        let valid = parts.next().is_none() && (1..=12).contains(&month) && (1..=31).contains(&day);
        valid.then(|| Self::from_ymd(year.into(), month.into(), day.into()))
    }

    /// The day of a proleptic Gregorian date (Howard Hinnant's
    /// `days_from_civil`).
    pub fn from_ymd(year: i64, month: i64, day: i64) -> Self {
        let year = if month <= 2 { year - 1 } else { year };
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        Self(era * 146_097 + day_of_era - 719_468)
    }

    /// The UTC day of a Unix time in milliseconds.
    pub fn from_unix_ms(ms: i64) -> Self {
        Self(ms.div_euclid(86_400_000))
    }

    /// (year, month 1–12, day 1–31), Howard Hinnant's `civil_from_days`.
    pub fn ymd(self) -> (i64, i64, i64) {
        let z = self.0 + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        (year, month, day)
    }

    /// The day `days` later.
    #[must_use]
    pub fn plus(self, days: i64) -> Self {
        Self(self.0 + days)
    }
}

#[cfg(test)]
mod tests {
    use super::Day;

    /// Day numbers from `date -j -u -f "%Y-%m-%d %H:%M:%S" "<d> 00:00:00" +%s` / 86400.
    #[test]
    fn days_and_back() {
        let cases = [
            ("1970-01-01", Some(0)),
            ("2000-02-29", Some(11_016)),
            ("2026-11-09", Some(20_766)),
            ("2026-11-23", Some(20_780)),
            ("2026-13-01", None),
            ("2026-11-00", None),
            ("2026-11", None),
            ("2026-11-09-01", None),
            ("2026-11-09T00:00:00Z", None),
            ("", None),
        ];
        for (text, expected) in cases {
            let day = Day::parse(text);
            assert_eq!(day.map(|d| d.0), expected, "{text}");
            if let Some(day) = day {
                let (y, m, d) = day.ymd();
                assert_eq!(format!("{y:04}-{m:02}-{d:02}"), text);
            }
        }
        assert_eq!(Day::from_unix_ms(1_794_942_161_000), Day(20_774));
        assert_eq!(Day::from_unix_ms(-1), Day(-1));
        assert_eq!(Day(20_766).plus(12).ymd(), (2026, 11, 21));
        assert_eq!(Day(20_780).plus(13).ymd(), (2026, 12, 6));
    }
}
