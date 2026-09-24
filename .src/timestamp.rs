//! When an item was archived, as text: RFC 3339 in UTC for a file, a
//! script or a `timestamptz` column — `2026-09-09T12:00:00Z` — and the
//! zone-less `DATETIME` form `MySQL` reads — `2026-09-09 12:00:00`.
//!
//! Five technologies each carried this until 2026-09-09, when it moved up
//! to the capability (ADR-0044); the calendar under it is
//! `codec::civil`, the estate's one, since 2026-09-24.

use std::time::SystemTime;

use codec::civil::CivilTime;

/// Now, in RFC 3339.
#[must_use]
pub fn now() -> String {
    CivilTime::now().rfc3339()
}

/// `at` as a `DATETIME` literal in UTC to the second — no `T`, no zone —
/// because a `DATETIME` keeps no zone and a literal with one is not read
/// by every `MySQL` and `MariaDB` version. A moment before the epoch is the
/// epoch itself.
#[must_use]
pub fn datetime_utc(at: SystemTime) -> String {
    let moment = CivilTime::from_system_time(at);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        moment.year, moment.month, moment.day, moment.hour, moment.minute, moment.second
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn a_known_instant_is_a_datetime_literal() {
        assert_eq!(datetime_utc(UNIX_EPOCH), "1970-01-01 00:00:00");
        let at = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
        assert_eq!(datetime_utc(at), "2020-09-13 12:26:40");
    }

    #[test]
    fn now_has_the_shape_of_a_timestamp() {
        let text = now();
        assert_eq!(text.len(), 20);
        assert!(text.ends_with('Z'));
        assert_eq!(&text[10..11], "T");
    }
}
