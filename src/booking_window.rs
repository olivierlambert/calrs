//! When an event type can be booked: the rolling booking horizon (migration
//! 063) intersected with the fixed bookable date range (migration 065), as
//! calendar days in the event type's own timezone. The web slot pages, the
//! booking handlers and the CLI all read the window through this module, so
//! the rules cannot drift between them.
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

/// How many days ahead any booking may be made. The booking handlers reject
/// later dates (`validate_date_not_too_far`), so the window ends here too:
/// a slot past it would be offered and then refused on submit.
pub const MAX_DAYS_AHEAD: i64 = 366;

/// Which calendar days an event type can be booked on, in its own timezone:
/// the rolling booking horizon (migration 063) intersected with the fixed
/// bookable date range (migration 065). Every bound is optional, and the
/// default is "no limit", the behaviour before either setting existed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BookingWindow {
    pub horizon_days: Option<i32>,
    pub bookable_from: Option<NaiveDate>,
    pub bookable_until: Option<NaiveDate>,
}

impl BookingWindow {
    /// The first and last bookable day, given today in the event type's
    /// timezone. The first day is never before today, and the last never
    /// after `MAX_DAYS_AHEAD`. When the last day is before the first, nothing
    /// is bookable (an expired range, a horizon that ends before the range
    /// starts, or a range more than a year out).
    pub fn effective_date_bounds(&self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let first = self.bookable_from.map_or(today, |from| from.max(today));
        let mut last = today
            .checked_add_signed(Duration::days(MAX_DAYS_AHEAD))
            .unwrap_or(NaiveDate::MAX);
        if let Some(horizon) = horizon_last_date(today.and_time(NaiveTime::MIN), self.horizon_days)
        {
            last = last.min(horizon);
        }
        if let Some(until) = self.bookable_until {
            last = last.min(until);
        }
        (first, last)
    }

    /// Whether an appointment starting on `date` (event type timezone) may
    /// be booked.
    pub fn contains(&self, date: NaiveDate, today: NaiveDate) -> bool {
        let (first, last) = self.effective_date_bounds(today);
        date >= first && date <= last
    }

    /// The range has ended: `bookable_until` is set and already behind us.
    /// An expired event type is hidden from public listings but stays
    /// enabled, so moving the end date forward brings it straight back.
    pub fn is_expired(&self, today: NaiveDate) -> bool {
        self.bookable_until.is_some_and(|until| today > until)
    }

    pub fn has_date_range(&self) -> bool {
        self.bookable_from.is_some() || self.bookable_until.is_some()
    }
}

/// Parse a stored or submitted `YYYY-MM-DD` date. Blank or malformed input
/// is `None`, which every bound treats as "no limit".
pub fn parse_optional_date(s: Option<&str>) -> Option<NaiveDate> {
    s.map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

/// The last host-local date a guest may book, for a given horizon.
/// `None` horizon means unlimited, so no date is ever past it. A horizon that
/// runs off the end of the representable calendar is unlimited for the same
/// reason, so `checked_add_signed` degrades to `None` rather than panicking
/// (`NaiveDate + TimeDelta` panics on overflow). The day loop in
/// `compute_slots_from_rules` treats such a horizon as unreachable too.
///
/// A negative horizon is only reachable by writing the column directly, since
/// `parse_optional_day_count` rejects one. It has to mean "nothing bookable",
/// so it is clamped to yesterday. Left to the arithmetic, a large negative
/// overflows to `None` and reads as unlimited, which is the wrong direction
/// to fail in.
pub fn horizon_last_date(now_host: NaiveDateTime, horizon: Option<i32>) -> Option<NaiveDate> {
    horizon.and_then(|days| {
        let date = now_host.date();
        if days < 0 {
            return date.pred_opt();
        }
        date.checked_add_signed(Duration::days(days as i64))
    })
}

/// The date in `to` on which host-local day `day` (in `from`) begins. The
/// slot calendar is drawn in the guest's dates while the window is in the
/// event type's, so a Tokyo fair's first morning can fall on the previous
/// day for a guest in America.
pub fn day_start_in(day: NaiveDate, from: Tz, to: Tz) -> NaiveDate {
    from.from_local_datetime(&day.and_time(NaiveTime::MIN))
        .earliest()
        .map_or(day, |t| t.with_timezone(&to).date_naive())
}

/// The date in `to` on which host-local day `day` (in `from`) ends.
pub fn day_end_in(day: NaiveDate, from: Tz, to: Tz) -> NaiveDate {
    let end = day
        .and_hms_opt(23, 59, 59)
        .unwrap_or(day.and_time(NaiveTime::MIN));
    from.from_local_datetime(&end)
        .latest()
        .map_or(day, |t| t.with_timezone(&to).date_naive())
}

/// Today's date in the event type's timezone, the day every window bound is
/// compared against.
pub fn today_in(tz: Tz) -> NaiveDate {
    Utc::now().with_timezone(&tz).date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn range(from: Option<&str>, until: Option<&str>) -> BookingWindow {
        BookingWindow {
            horizon_days: None,
            bookable_from: from.map(d),
            bookable_until: until.map(d),
        }
    }

    const TODAY: &str = "2026-11-02";

    #[test]
    fn default_window_is_unbounded_from_today() {
        let w = BookingWindow::default();
        assert_eq!(
            w.effective_date_bounds(d(TODAY)),
            (d(TODAY), d(TODAY) + Duration::days(MAX_DAYS_AHEAD)),
            "without limits the window still ends where booking ends"
        );
        assert!(w.contains(d("2027-06-01"), d(TODAY)));
        assert!(
            !w.contains(d("2030-01-01"), d(TODAY)),
            "past the one-year cap"
        );
        assert!(
            !w.contains(d("2026-11-01"), d(TODAY)),
            "the past is never bookable"
        );
        assert!(!w.is_expired(d(TODAY)));
        assert!(!w.has_date_range());
    }

    #[test]
    fn both_bounds_are_inclusive() {
        let w = range(Some("2026-11-10"), Some("2026-11-12"));
        assert!(!w.contains(d("2026-11-09"), d(TODAY)));
        assert!(w.contains(d("2026-11-10"), d(TODAY)));
        assert!(w.contains(d("2026-11-12"), d(TODAY)));
        assert!(!w.contains(d("2026-11-13"), d(TODAY)));
    }

    #[test]
    fn only_from_has_no_upper_bound() {
        let w = range(Some("2026-11-10"), None);
        assert_eq!(w.effective_date_bounds(d(TODAY)).0, d("2026-11-10"));
        assert!(w.contains(d("2027-06-01"), d(TODAY)));
        assert!(!w.contains(d("2026-11-09"), d(TODAY)));
    }

    #[test]
    fn only_until_has_no_lower_bound_beyond_today() {
        let w = range(None, Some("2026-11-12"));
        assert_eq!(
            w.effective_date_bounds(d(TODAY)),
            (d(TODAY), d("2026-11-12"))
        );
        assert!(w.contains(d(TODAY), d(TODAY)));
        assert!(!w.contains(d("2026-11-13"), d(TODAY)));
    }

    #[test]
    fn a_running_range_starts_today_not_at_its_first_day() {
        let w = range(Some("2026-10-01"), Some("2026-11-12"));
        assert_eq!(w.effective_date_bounds(d(TODAY)).0, d(TODAY));
    }

    #[test]
    fn horizon_shorter_than_the_range_cuts_its_end() {
        let w = BookingWindow {
            horizon_days: Some(9),
            ..range(Some("2026-11-10"), Some("2026-11-12"))
        };
        assert_eq!(
            w.effective_date_bounds(d(TODAY)),
            (d("2026-11-10"), d("2026-11-11"))
        );
        assert!(!w.contains(d("2026-11-12"), d(TODAY)));
    }

    #[test]
    fn horizon_ending_before_the_range_leaves_nothing_bookable() {
        let w = BookingWindow {
            horizon_days: Some(3),
            ..range(Some("2026-11-10"), Some("2026-11-12"))
        };
        let (first, last) = w.effective_date_bounds(d(TODAY));
        assert!(last < first);
        for day in ["2026-11-05", "2026-11-10", "2026-11-12"] {
            assert!(!w.contains(d(day), d(TODAY)), "{day} must not be bookable");
        }
    }

    #[test]
    fn expiry_starts_the_day_after_the_end_date() {
        let w = range(Some("2026-11-10"), Some("2026-11-12"));
        assert!(
            !w.is_expired(d("2026-11-12")),
            "the last day is still bookable"
        );
        assert!(w.is_expired(d("2026-11-13")));
        assert!(!range(Some("2026-11-10"), None).is_expired(d("2030-01-01")));
    }

    #[test]
    fn a_range_more_than_a_year_out_is_not_bookable_yet() {
        let w = range(Some("2028-03-01"), Some("2028-03-03"));
        let (first, last) = w.effective_date_bounds(d(TODAY));
        assert!(
            last < first,
            "slots that the booking form would refuse are not offered"
        );
    }

    #[test]
    fn day_bounds_follow_the_guest_calendar() {
        let tokyo: Tz = "Asia/Tokyo".parse().unwrap();
        let new_york: Tz = "America/New_York".parse().unwrap();
        // Tokyo's 1 November starts on 31 October in New York...
        assert_eq!(
            day_start_in(d("2026-11-01"), tokyo, new_york),
            d("2026-10-31")
        );
        // ...and ends there on 1 November.
        assert_eq!(
            day_end_in(d("2026-11-01"), tokyo, new_york),
            d("2026-11-01")
        );
        // Same timezone: the day itself.
        assert_eq!(day_start_in(d("2026-11-01"), tokyo, tokyo), d("2026-11-01"));
    }

    #[test]
    fn parse_optional_date_accepts_iso_dates_only() {
        assert_eq!(
            parse_optional_date(Some("2026-11-10")),
            Some(d("2026-11-10"))
        );
        assert_eq!(
            parse_optional_date(Some(" 2026-11-10 ")),
            Some(d("2026-11-10"))
        );
        assert_eq!(parse_optional_date(Some("")), None);
        assert_eq!(parse_optional_date(Some("10.11.2026")), None);
        assert_eq!(parse_optional_date(None), None);
    }
}
