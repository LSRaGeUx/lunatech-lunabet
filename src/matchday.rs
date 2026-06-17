//! Shared definition of a "matchday" window.
//!
//! WC2026 is played in North America, so games kick off in the European
//! afternoon and run through the early hours of the next morning. We group them
//! into a matchday that spans 15:00 CEST to 08:00 CEST the next morning (13:00
//! UTC to 06:00 UTC), keyed by the CEST calendar date it starts on. An
//! early-morning game (before 08:00 CEST) therefore belongs to the *previous*
//! calendar date's matchday, matching how a late-night kickoff is felt to be
//! part of "that evening".
//!
//! The Today screen, the daily recap digest, the morning preview email and the
//! player-of-the-day all share this window so they agree on which matches
//! belong to a given day.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

/// CEST is UTC+2 (summer time, in effect for the whole tournament).
pub const CEST_OFFSET_HOURS: i64 = 2;

/// The matchday starting on the CEST calendar date `date`, as a half-open UTC
/// range `[date 15:00 CEST, date+1 08:00 CEST)` == `[date 13:00 UTC, date+1
/// 06:00 UTC)`.
pub fn window(date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_utc_hour = (15 - CEST_OFFSET_HOURS) as u32; // 13
    let end_utc_hour = (8 - CEST_OFFSET_HOURS) as u32; // 6
    let next = date.succ_opt().unwrap_or(date);
    let start = Utc.from_utc_datetime(&date.and_hms_opt(start_utc_hour, 0, 0).unwrap());
    let end = Utc.from_utc_datetime(&next.and_hms_opt(end_utc_hour, 0, 0).unwrap());
    (start, end)
}

/// The CEST calendar date in effect at `now`. Used to pick "today's" matchday.
pub fn cest_date(now: DateTime<Utc>) -> NaiveDate {
    (now + Duration::hours(CEST_OFFSET_HOURS)).date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn window_runs_from_afternoon_to_next_morning() {
        let (start, end) = window(d(2026, 6, 16));
        assert_eq!(start, utc("2026-06-16T13:00:00Z")); // 15:00 CEST
        assert_eq!(end, utc("2026-06-17T06:00:00Z")); // 08:00 CEST next day
    }

    #[test]
    fn early_morning_match_belongs_to_previous_days_matchday() {
        // A game kicking off at 06:00 CEST (04:00 UTC) on the 17th is part of
        // the matchday that started the afternoon of the 16th, not the 17th.
        let ko = utc("2026-06-17T04:00:00Z");

        let (s16, e16) = window(d(2026, 6, 16));
        assert!(ko >= s16 && ko < e16, "should fall in the 16th's matchday");

        let (s17, _e17) = window(d(2026, 6, 17));
        assert!(ko < s17, "should be before the 17th's matchday starts");
    }
}
