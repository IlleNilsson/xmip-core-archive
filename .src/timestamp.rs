//! When an item was archived, as text: RFC 3339 in UTC for a file, a
//! script or a `timestamptz` column — `2026-09-09T12:00:00Z` — and the
//! zone-less `DATETIME` form `MariaDB` reads — `2026-09-09 12:00:00`.
//!
//! Five technologies each carried this until 2026-09-09, when it moved up
//! to the capability (ADR-0044); the calendar under it is
//! `codec::civil`, the estate's one, since 2026-09-24. Each form takes its
//! moment as seconds since the epoch; [`now`] reads it off the one clock
//! (`xcore::Clock`).

use codec::civil::CivilTime;
use xcore::{Clock, SystemClock};

/// Now, in RFC 3339.
#[must_use]
pub fn now() -> String {
    rfc3339(SystemClock.unix_seconds())
}

/// The moment `seconds` after the epoch in RFC 3339, in UTC to the second.
#[must_use]
pub fn rfc3339(seconds: i64) -> String {
    CivilTime::from_unix(seconds).rfc3339()
}

/// The moment `seconds` after the epoch as a `DATETIME` literal in UTC to
/// the second — no `T`, no zone — because a `DATETIME` keeps no zone and a
/// literal with one is not read by every `MariaDB` version.
#[must_use]
pub fn datetime_utc(seconds: i64) -> String {
    let moment = CivilTime::from_unix(seconds);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        moment.year, moment.month, moment.day, moment.hour, moment.minute, moment.second
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_instant_is_a_datetime_literal() {
        assert_eq!(datetime_utc(0), "1970-01-01 00:00:00");
        assert_eq!(datetime_utc(1_600_000_000), "2020-09-13 12:26:40");
        assert_eq!(rfc3339(1_600_000_000), "2020-09-13T12:26:40Z");
    }

    #[test]
    fn now_has_the_shape_of_a_timestamp() {
        let text = now();
        assert_eq!(text.len(), 20);
        assert!(text.ends_with('Z'));
        assert_eq!(&text[10..11], "T");
    }
}
