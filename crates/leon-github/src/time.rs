//! GitHub's UTC timestamps (`2026-11-17T19:02:41Z`) as Unix milliseconds.

use leon_domain::Day;

/// Unix milliseconds of a GitHub `DateTime`; fractions of a second are
/// dropped. `None` when it is not one.
pub(crate) fn unix_ms(text: &str) -> Option<i64> {
    let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
    let days = Day::parse(date)?.0;
    let mut parts = time
        .split('.')
        .next()?
        .split(':')
        .map(|p| p.parse::<u32>().ok());
    let [hour, minute, second] = [parts.next()??, parts.next()??, parts.next()??].map(i64::from);
    let valid = parts.next().is_none() && hour < 24 && minute < 60 && second <= 60;
    valid.then_some((((days * 24 + hour) * 60 + minute) * 60 + second) * 1000)
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
