//! When an item was archived, as text: RFC 3339 in UTC for a file, a
//! script or a `timestamptz` column — `2026-09-09T12:00:00Z` — and the
//! zone-less `DATETIME` form `MySQL` reads — `2026-09-09 12:00:00`.
//!
//! Written by hand from the seconds since the epoch, because a date is the
//! one thing an archive needs from a calendar and a dependency for it would
//! be the largest thing in the crate. Five technologies each carried this
//! until 2026-09-09; it moved up to the capability (ADR-0044).

use std::time::{SystemTime, UNIX_EPOCH};

const SECONDS_A_DAY: i64 = 86_400;

/// Now, in RFC 3339.
#[must_use]
pub fn now() -> String {
    rfc3339_utc(SystemTime::now())
}

/// `at` as RFC 3339 in UTC to the whole second. A moment before the epoch,
/// which no clock this runs under reports, is written as the epoch itself.
#[must_use]
pub fn rfc3339_utc(at: SystemTime) -> String {
    let (year, month, day, of_day) = civil(at);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

/// `at` as a `DATETIME` literal in UTC to the second — no `T`, no zone —
/// because a `DATETIME` keeps no zone and a literal with one is not read
/// by every `MySQL` and `MariaDB` version.
#[must_use]
pub fn datetime_utc(at: SystemTime) -> String {
    let (year, month, day, of_day) = civil(at);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

/// Year, month, day and the second of the day for `at`.
fn civil(at: SystemTime) -> (i64, i64, i64, i64) {
    let seconds = at
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(seconds.div_euclid(SECONDS_A_DAY));
    (year, month, day, seconds.rem_euclid(SECONDS_A_DAY))
}

/// The proleptic Gregorian date `days` after 1970-01-01, by the era
/// arithmetic every calendar library uses: four-hundred-year eras of
/// 146 097 days, the year within the era, the day within a March-first year.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let of_era = shifted.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_epoch_is_the_first_moment_of_1970() {
        assert_eq!(rfc3339_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(datetime_utc(UNIX_EPOCH), "1970-01-01 00:00:00");
    }

    #[test]
    fn a_known_instant_is_written_both_ways() {
        let at = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        assert_eq!(rfc3339_utc(at), "2020-09-13T12:26:40Z");
        assert_eq!(datetime_utc(at), "2020-09-13 12:26:40");
    }

    #[test]
    // The number is an epoch second count, and only reads as a moment in
    // seconds; from_hours would hide which moment it is.
    #[allow(clippy::duration_suboptimal_units)]
    fn a_leap_day_is_a_real_day() {
        let at = UNIX_EPOCH + Duration::from_secs(1_709_164_800);
        assert_eq!(rfc3339_utc(at), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn now_has_the_shape_of_a_timestamp() {
        let text = now();
        assert_eq!(text.len(), 20);
        assert!(text.ends_with('Z'));
        assert_eq!(&text[10..11], "T");
    }
}
