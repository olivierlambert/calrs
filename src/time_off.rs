//! Time off (#46): periods a user takes no bookings, whatever the event type.
//!
//! A period belongs to one user and blocks them everywhere slots are
//! computed: `fetch_busy_times_for_user_opts` in `src/web/availability.rs`
//! adds [`busy_intervals`] to the user's busy times, so personal event types,
//! team round robin (the member is skipped), collective (the slot is gone),
//! the submit-time re-check, reschedules and the approval re-check all see
//! it. The troubleshoot view and the CLI read it here too.
//!
//! Periods are stored as UTC instants, end exclusive, with the zone they were
//! entered in so they can be shown back as typed. The label is private to its
//! owner: guests only ever see that a time is not available.

use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use sqlx::SqlitePool;

/// Longest label accepted, in characters.
pub const LABEL_MAX: usize = 200;

/// Longest period accepted, in days. Longer than that is an account to
/// disable, not time off.
pub const MAX_DAYS: i64 = 366;

/// Ended periods are kept this long, then pruned by the background loop.
pub const KEEP_ENDED_DAYS: i64 = 30;

const STORED: &str = "%Y-%m-%dT%H:%M:%SZ";

/// One stored period.
#[derive(Debug, Clone)]
pub struct Period {
    pub id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// The zone it was entered in (UTC when the stored name no longer parses).
    pub tz: Tz,
    pub timezone: String,
    pub label: Option<String>,
}

impl Period {
    /// Start as wall-clock time in the zone it was entered in.
    pub fn local_start(&self) -> NaiveDateTime {
        self.start.with_timezone(&self.tz).naive_local()
    }

    /// End (exclusive) as wall-clock time in the zone it was entered in.
    pub fn local_end(&self) -> NaiveDateTime {
        self.end.with_timezone(&self.tz).naive_local()
    }

    /// Whole days only: starts and ends at midnight where it was entered.
    pub fn whole_days(&self) -> bool {
        self.local_start().time() == NaiveTime::MIN && self.local_end().time() == NaiveTime::MIN
    }

    pub fn in_progress(&self, now: DateTime<Utc>) -> bool {
        self.start <= now && now < self.end
    }
}

/// Why a submitted period was refused. Each maps to one message key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputError {
    Date,
    Order,
    TooLong,
    Past,
    Label,
}

impl InputError {
    pub fn key(self) -> &'static str {
        match self {
            InputError::Date => "time-off-error-date",
            InputError::Order => "time-off-error-order",
            InputError::TooLong => "time-off-error-too-long",
            InputError::Past => "time-off-error-past",
            InputError::Label => "time-off-error-label",
        }
    }

    /// The same refusal in English, for the CLI.
    pub fn message(self) -> &'static str {
        match self {
            InputError::Date => "enter dates as YYYY-MM-DD and times as HH:MM",
            InputError::Order => "the end must be after the start",
            InputError::TooLong => "a period can be at most a year long",
            InputError::Past => "this period has already ended",
            InputError::Label => "the note can be at most 200 characters",
        }
    }
}

/// A wall-clock time in `tz` as an instant. In a repeated hour the start
/// takes the first occurrence and the end the second, so the period covers
/// both; in a skipped hour both move to the first time that exists.
fn to_utc(tz: Tz, local: NaiveDateTime, is_end: bool) -> Option<DateTime<Utc>> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(t) => Some(t.with_timezone(&Utc)),
        LocalResult::Ambiguous(first, second) => {
            Some(if is_end { second } else { first }.with_timezone(&Utc))
        }
        LocalResult::None => (1..=16)
            .map(|step| local + Duration::minutes(15 * step))
            .find_map(|t| tz.from_local_datetime(&t).earliest())
            .map(|t| t.with_timezone(&Utc)),
    }
}

fn parse_time(s: &str) -> Result<Option<NaiveTime>, InputError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    NaiveTime::parse_from_str(s, "%H:%M")
        .map(Some)
        .map_err(|_| InputError::Date)
}

/// Turn what was typed into a period. `end_date` empty means the start
/// date. An empty start time means the start of the day, an empty end time
/// the end of the end date, so two dates and no times block whole days,
/// both included.
pub fn parse_period(
    start_date: &str,
    start_time: &str,
    end_date: &str,
    end_time: &str,
    tz: Tz,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), InputError> {
    let start_day =
        NaiveDate::parse_from_str(start_date.trim(), "%Y-%m-%d").map_err(|_| InputError::Date)?;
    let end_day = if end_date.trim().is_empty() {
        start_day
    } else {
        NaiveDate::parse_from_str(end_date.trim(), "%Y-%m-%d").map_err(|_| InputError::Date)?
    };
    let start_local = start_day.and_time(parse_time(start_time)?.unwrap_or(NaiveTime::MIN));
    let end_local = match parse_time(end_time)? {
        Some(t) => end_day.and_time(t),
        None => end_day
            .succ_opt()
            .ok_or(InputError::Date)?
            .and_time(NaiveTime::MIN),
    };
    if end_local <= start_local {
        return Err(InputError::Order);
    }
    if end_local - start_local > Duration::days(MAX_DAYS) {
        return Err(InputError::TooLong);
    }
    let start = to_utc(tz, start_local, false).ok_or(InputError::Date)?;
    let end = to_utc(tz, end_local, true).ok_or(InputError::Date)?;
    if end <= start {
        return Err(InputError::Order);
    }
    if end <= now {
        return Err(InputError::Past);
    }
    Ok((start, end))
}

/// Trim the label; empty means none.
pub fn normalize_label(label: &str) -> Result<Option<String>, InputError> {
    let label = label.trim();
    if label.is_empty() {
        Ok(None)
    } else if label.chars().count() > LABEL_MAX {
        Err(InputError::Label)
    } else {
        Ok(Some(label.to_string()))
    }
}

type Row = (String, String, String, String, Option<String>);

fn from_row((id, start, end, timezone, label): Row) -> Option<Period> {
    let parse = |s: &str| {
        NaiveDateTime::parse_from_str(s, STORED)
            .ok()
            .map(|t| t.and_utc())
    };
    Some(Period {
        id,
        start: parse(&start)?,
        end: parse(&end)?,
        tz: timezone.parse().unwrap_or(Tz::UTC),
        timezone,
        label,
    })
}

/// Store a period for `user_id`. Returns its id.
pub async fn add(
    pool: &SqlitePool,
    user_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timezone: &str,
    label: Option<&str>,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO user_time_off (id, user_id, start_at, end_at, timezone, label) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(start.format(STORED).to_string())
    .bind(end.format(STORED).to_string())
    .bind(timezone)
    .bind(label)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Delete one of `user_id`'s periods. A period of someone else is left
/// alone and reported as not found.
pub async fn delete(pool: &SqlitePool, user_id: &str, id: &str) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM user_time_off WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// `user_id`'s periods that have not ended yet, soonest first.
pub async fn upcoming(
    pool: &SqlitePool,
    user_id: &str,
    now: DateTime<Utc>,
) -> Result<Vec<Period>, sqlx::Error> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, start_at, end_at, timezone, label FROM user_time_off \
         WHERE user_id = ? AND end_at > ? ORDER BY start_at, end_at",
    )
    .bind(user_id)
    .bind(now.format(STORED).to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(from_row).collect())
}

/// `user_id`'s periods overlapping `[from, to)`.
pub async fn overlapping(
    pool: &SqlitePool,
    user_id: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<Period>, sqlx::Error> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, start_at, end_at, timezone, label FROM user_time_off \
         WHERE user_id = ? AND start_at < ? AND end_at > ? ORDER BY start_at",
    )
    .bind(user_id)
    .bind(to.format(STORED).to_string())
    .bind(from.format(STORED).to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().filter_map(from_row).collect())
}

/// `user_id`'s time off around a window given as wall-clock times in `tz`,
/// as busy intervals in `tz`: the shape the slot engine works in. The window
/// is widened by two days, so its zone never needs resolving here; intervals
/// outside it are harmless to the overlap checks. A read failure is logged
/// and blocks nothing, like the other busy-time reads.
pub async fn busy_intervals(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    tz: Tz,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let from = window_start.and_utc() - Duration::days(2);
    let to = window_end.and_utc() + Duration::days(2);
    match overlapping(pool, user_id, from, to).await {
        Ok(periods) => periods
            .iter()
            .map(|p| {
                (
                    p.start.with_timezone(&tz).naive_local(),
                    p.end.with_timezone(&tz).naive_local(),
                )
            })
            .collect(),
        Err(error) => {
            tracing::error!(%error, %user_id, "cannot read time off");
            Vec::new()
        }
    }
}

/// Delete periods that ended more than [`KEEP_ENDED_DAYS`] ago.
pub async fn prune(pool: &SqlitePool, now: DateTime<Utc>) -> Result<u64, sqlx::Error> {
    let cutoff = (now - Duration::days(KEEP_ENDED_DAYS))
        .format(STORED)
        .to_string();
    let done = sqlx::query("DELETE FROM user_time_off WHERE end_at < ?")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(done.rows_affected())
}

#[cfg(test)]
mod tests;
