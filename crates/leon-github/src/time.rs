//! GitHub's UTC timestamps (`2026-11-17T19:02:41Z`) as Unix milliseconds.

/// Unix milliseconds of a GitHub `DateTime`; fractions of a second are
/// dropped. `None` when it is not one.
pub(crate) fn unix_ms(text: &str) -> Option<i64> {
    let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
    let [year, month, day] = numbers(date, '-')?;
    let [hour, minute, second] = numbers(time.split('.').next()?, ':')?;
    let valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && minute < 60
        && second <= 60;
    let days = days_from_civil(year, month, day);
    valid.then_some((((days * 24 + hour) * 60 + minute) * 60 + second) * 1000)
}

/// Exactly three non-negative numbers separated by `sep`.
fn numbers(text: &str, sep: char) -> Option<[i64; 3]> {
    let mut parts = text.split(sep).map(|p| p.parse::<u32>().ok());
    let found = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(found.map(i64::from))
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::unix_ms;

    /// Expected values from `date -j -u -f %Y-%m-%dT%H:%M:%S <t> +%s`.
    #[test]
    fn timestamps() {
        let cases = [
            ("1970-01-01T00:00:00Z", Some(0)),
            ("2000-02-29T12:00:00Z", Some(951_825_600_000)),
            ("2026-11-17T19:02:41Z", Some(1_794_942_161_000)),
            ("2026-11-17T19:02:41.250Z", Some(1_794_942_161_000)),
            ("2026-11-17T19:02:41", None),
            ("2026-11-17 19:02:41Z", None),
            ("2026-13-01T00:00:00Z", None),
            ("2026-11-17T24:00:00Z", None),
            ("2026-11-17-01T00:00:00Z", None),
            ("-2026-11-17T00:00:00Z", None),
            ("", None),
        ];
        for (text, expected) in cases {
            assert_eq!(unix_ms(text), expected, "{text}");
        }
    }
}
