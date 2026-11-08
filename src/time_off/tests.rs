use super::*;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

async fn database() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::from_str("sqlite::memory:")
                .unwrap()
                .foreign_keys(true),
        )
        .await
        .unwrap();
    crate::db::migrate(&pool).await.unwrap();
    pool
}

async fn user(pool: &SqlitePool, email: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled) \
         VALUES (?, ?, 'U', 'user', 'local', ?, 1)",
    )
    .bind(&id)
    .bind(email)
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn local(s: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").unwrap()
}

const NOW: &str = "2026-11-07T12:00:00Z";
const PARIS: Tz = chrono_tz::Europe::Paris;

#[test]
fn two_dates_without_times_block_both_days() {
    let (start, end) = parse_period("2026-12-21", "", "2026-12-24", "", PARIS, utc(NOW)).unwrap();
    // Paris is UTC+1 in December: midnight there is 23:00 UTC the day before.
    assert_eq!(start, utc("2026-12-20T23:00:00Z"));
    assert_eq!(end, utc("2026-12-24T23:00:00Z"));
}

#[test]
fn one_date_blocks_that_day() {
    let (start, end) = parse_period("2026-12-21", "", "", "", Tz::UTC, utc(NOW)).unwrap();
    assert_eq!(start, utc("2026-12-21T00:00:00Z"));
    assert_eq!(end, utc("2026-12-22T00:00:00Z"));
}

#[test]
fn times_narrow_the_period() {
    let (start, end) = parse_period(
        "2026-12-21",
        "14:00",
        "2026-12-21",
        "17:30",
        PARIS,
        utc(NOW),
    )
    .unwrap();
    assert_eq!(start, utc("2026-12-21T13:00:00Z"));
    assert_eq!(end, utc("2026-12-21T16:30:00Z"));
    // Only a start time: until the end of the end date.
    let (_, end) = parse_period("2026-12-21", "14:00", "", "", PARIS, utc(NOW)).unwrap();
    assert_eq!(end, utc("2026-12-21T23:00:00Z"));
}

#[test]
fn bad_input_is_refused_with_its_reason() {
    let now = utc(NOW);
    let parse = |a, b, c, d| parse_period(a, b, c, d, PARIS, now).unwrap_err();
    assert_eq!(parse("", "", "", ""), InputError::Date);
    assert_eq!(parse("21/12/2026", "", "", ""), InputError::Date);
    assert_eq!(parse("2026-12-21", "9h", "", ""), InputError::Date);
    assert_eq!(parse("2026-12-21", "", "2026-12-20", ""), InputError::Order);
    assert_eq!(
        parse("2026-12-21", "15:00", "2026-12-21", "15:00"),
        InputError::Order
    );
    assert_eq!(
        parse("2026-12-21", "", "2028-01-01", ""),
        InputError::TooLong
    );
    assert_eq!(parse("2026-11-01", "", "2026-11-06", ""), InputError::Past);
    // A period still in progress is accepted.
    assert!(parse_period("2026-11-01", "", "2026-11-07", "", PARIS, now).is_ok());
    for e in [
        InputError::Date,
        InputError::Order,
        InputError::TooLong,
        InputError::Past,
        InputError::Label,
    ] {
        assert!(e.key().starts_with("time-off-error-"));
        assert!(!e.message().is_empty());
    }
}

#[test]
fn clock_changes_keep_the_whole_period() {
    let now = utc("2026-01-01T00:00:00Z");
    // 02:30 does not exist in Paris on 29 March 2026: it moves to 03:00.
    let (start, _) =
        parse_period("2026-03-29", "02:30", "2026-03-29", "05:00", PARIS, now).unwrap();
    assert_eq!(start, utc("2026-03-29T01:00:00Z"));
    // Any skipped minute lands on the first one after the gap, 03:00, for
    // a start and for an end alike.
    let (start, _) =
        parse_period("2026-03-29", "02:01", "2026-03-29", "05:00", PARIS, now).unwrap();
    assert_eq!(start, utc("2026-03-29T01:00:00Z"));
    let (_, end) = parse_period("2026-03-29", "01:00", "2026-03-29", "02:45", PARIS, now).unwrap();
    assert_eq!(end, utc("2026-03-29T01:00:00Z"));
    // 02:00 to 02:30 happens twice on 25 October 2026: the start takes the
    // first occurrence, the end the second, so both are covered.
    let (start, end) =
        parse_period("2026-10-25", "02:00", "2026-10-25", "02:30", PARIS, now).unwrap();
    assert_eq!(start, utc("2026-10-25T00:00:00Z"));
    assert_eq!(end, utc("2026-10-25T01:30:00Z"));
}

#[test]
fn labels_are_trimmed_and_capped() {
    assert_eq!(normalize_label("  "), Ok(None));
    assert_eq!(normalize_label(" Ski "), Ok(Some("Ski".into())));
    assert_eq!(
        normalize_label(&"é".repeat(LABEL_MAX)).map(|l| l.map(|l| l.chars().count())),
        Ok(Some(LABEL_MAX))
    );
    assert_eq!(
        normalize_label(&"x".repeat(LABEL_MAX + 1)),
        Err(InputError::Label)
    );
}

#[tokio::test]
async fn periods_are_shown_back_as_entered() {
    let pool = database().await;
    let alice = user(&pool, "alice@example.com").await;
    let (start, end) = parse_period("2026-12-21", "", "2026-12-24", "", PARIS, utc(NOW)).unwrap();
    add(&pool, &alice, start, end, "Europe/Paris", Some("Ski"))
        .await
        .unwrap();
    let periods = upcoming(&pool, &alice, utc(NOW)).await.unwrap();
    assert_eq!(periods.len(), 1);
    let p = &periods[0];
    assert_eq!(p.local_start(), local("2026-12-21T00:00"));
    assert_eq!(p.local_end(), local("2026-12-25T00:00"));
    assert!(p.whole_days());
    assert_eq!(p.label.as_deref(), Some("Ski"));
    assert!(!p.in_progress(utc(NOW)));
    assert!(p.in_progress(utc("2026-12-22T10:00:00Z")));
}

#[tokio::test]
async fn a_user_deletes_only_their_own_periods() {
    let pool = database().await;
    let alice = user(&pool, "alice@example.com").await;
    let bob = user(&pool, "bob@example.com").await;
    let id = add(
        &pool,
        &alice,
        utc("2026-12-21T00:00:00Z"),
        utc("2026-12-22T00:00:00Z"),
        "UTC",
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!delete(&pool, &bob, &id).await.unwrap());
    assert_eq!(upcoming(&pool, &alice, utc(NOW)).await.unwrap().len(), 1);
    assert!(upcoming(&pool, &bob, utc(NOW)).await.unwrap().is_empty());
    assert!(delete(&pool, &alice, &id).await.unwrap());
    assert!(upcoming(&pool, &alice, utc(NOW)).await.unwrap().is_empty());
}

#[tokio::test]
async fn busy_intervals_are_in_the_callers_zone() {
    let pool = database().await;
    let alice = user(&pool, "alice@example.com").await;
    let bob = user(&pool, "bob@example.com").await;
    add(
        &pool,
        &alice,
        utc("2026-12-21T13:00:00Z"),
        utc("2026-12-21T16:00:00Z"),
        "Europe/Paris",
        None,
    )
    .await
    .unwrap();
    let ny = chrono_tz::America::New_York;
    let busy = busy_intervals(
        &pool,
        &alice,
        local("2026-12-21T00:00"),
        local("2026-12-21T23:59"),
        ny,
    )
    .await;
    assert_eq!(
        busy,
        vec![(local("2026-12-21T08:00"), local("2026-12-21T11:00"))]
    );
    // Someone else's time off never counts.
    assert!(busy_intervals(
        &pool,
        &bob,
        local("2026-12-21T00:00"),
        local("2026-12-21T23:59"),
        ny
    )
    .await
    .is_empty());
    // Far outside the window: not read.
    assert!(busy_intervals(
        &pool,
        &alice,
        local("2027-02-01T00:00"),
        local("2027-02-01T23:59"),
        ny
    )
    .await
    .is_empty());
}

#[tokio::test]
async fn prune_drops_long_ended_periods_only() {
    let pool = database().await;
    let alice = user(&pool, "alice@example.com").await;
    let now = utc(NOW);
    for (s, e) in [
        ("2026-09-01T00:00:00Z", "2026-09-02T00:00:00Z"),
        ("2026-11-01T00:00:00Z", "2026-11-02T00:00:00Z"),
        ("2026-12-01T00:00:00Z", "2026-12-02T00:00:00Z"),
    ] {
        add(&pool, &alice, utc(s), utc(e), "UTC", None)
            .await
            .unwrap();
    }
    assert_eq!(prune(&pool, now).await.unwrap(), 1);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_time_off")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 2);
    // Ended but recent: kept, though no longer listed.
    assert_eq!(upcoming(&pool, &alice, now).await.unwrap().len(), 1);
}

#[tokio::test]
async fn deleting_the_user_deletes_their_time_off() {
    let pool = database().await;
    let alice = user(&pool, "alice@example.com").await;
    add(
        &pool,
        &alice,
        utc("2026-12-21T00:00:00Z"),
        utc("2026-12-22T00:00:00Z"),
        "UTC",
        None,
    )
    .await
    .unwrap();
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&alice)
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_time_off")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

/// The refusal keys reach Fluent through a variable, where the Rust key
/// guard in `src/i18n.rs` cannot see them.
#[test]
fn every_refusal_key_is_translated() {
    for key in [
        InputError::Date.key(),
        InputError::Order.key(),
        InputError::TooLong.key(),
        InputError::Past.key(),
        InputError::Label.key(),
        "time-off-error-too-many",
    ] {
        assert_ne!(crate::i18n::translate("en", key, None), key);
    }
}

#[test]
fn far_years_are_refused() {
    let now = utc(NOW);
    // A five-digit year would be stored as "+20261-..." and block nothing.
    assert_eq!(
        parse_period("20261-12-21", "", "", "", PARIS, now),
        Err(InputError::Date)
    );
    assert_eq!(
        parse_period("2026-12-21", "", "20261-12-21", "", PARIS, now),
        Err(InputError::Date)
    );
    // More than about five years ahead is a typo too.
    assert_eq!(
        parse_period("2032-01-01", "", "", "", PARIS, now),
        Err(InputError::Date)
    );
    assert!(parse_period("2031-11-01", "", "", "", PARIS, now).is_ok());
}

#[test]
fn whole_days_survive_a_clock_change_at_midnight() {
    // Chile moves its clocks forward at midnight: 6 September 2026 starts
    // at 01:00 in Santiago.
    let santiago = chrono_tz::America::Santiago;
    let now = utc("2026-08-01T00:00:00Z");
    let (start, end) = parse_period("2026-09-06", "", "2026-09-07", "", santiago, now).unwrap();
    let p = Period {
        id: String::new(),
        start,
        end,
        tz: santiago,
        timezone: "America/Santiago".into(),
        label: None,
    };
    assert_eq!(p.local_start(), local("2026-09-06T01:00"));
    assert!(p.whole_days(), "a day that starts at 01:00 is still a day");
    // A real time on that day is not a day boundary.
    let (start, end) =
        parse_period("2026-09-06", "14:00", "2026-09-07", "", santiago, now).unwrap();
    let p = Period { start, end, ..p };
    assert!(!p.starts_on_day_boundary());
    assert!(p.ends_on_day_boundary());
}

#[tokio::test]
async fn a_user_holds_at_most_the_cap_of_upcoming_periods() {
    let pool = database().await;
    let alice = user(&pool, "alice@example.com").await;
    let now = utc(NOW);
    // Ended periods do not count.
    add_capped(
        &pool,
        &alice,
        utc("2026-10-01T00:00:00Z"),
        utc("2026-10-02T00:00:00Z"),
        "UTC",
        None,
        now,
    )
    .await
    .unwrap()
    .unwrap();
    for i in 0..MAX_UPCOMING {
        let start = utc("2027-01-01T00:00:00Z") + Duration::days(i);
        add_capped(
            &pool,
            &alice,
            start,
            start + Duration::hours(1),
            "UTC",
            None,
            now,
        )
        .await
        .unwrap()
        .expect("under the cap");
    }
    let start = utc("2028-01-01T00:00:00Z");
    let over = add_capped(
        &pool,
        &alice,
        start,
        start + Duration::hours(1),
        "UTC",
        None,
        now,
    )
    .await
    .unwrap();
    assert!(over.is_none(), "the cap refuses the next one");
    // Someone else is not affected.
    let bob = user(&pool, "bob@example.com").await;
    assert!(add_capped(
        &pool,
        &bob,
        start,
        start + Duration::hours(1),
        "UTC",
        None,
        now
    )
    .await
    .unwrap()
    .is_some());
}

#[test]
fn local_ranges_cover_the_repeated_hour() {
    // 25 October 2026 in Paris: 03:00 CEST becomes 02:00 CET. 00:30Z is
    // 02:30 CEST and 01:15Z is 02:15 CET, so the plain local ends reverse.
    let (s, e) = local_range(
        utc("2026-10-25T00:30:00Z"),
        utc("2026-10-25T01:15:00Z"),
        PARIS,
    );
    assert!(s < e, "{s} to {e}");
    // The hull of what it covers: the whole repeated hour, no more.
    assert_eq!(
        (s, e),
        (local("2026-10-25T02:00"), local("2026-10-25T03:00"))
    );
    // Starting before the repeated hour and ending in its first occurrence
    // (02:30 CEST) keeps both ends: nothing of the second one is covered.
    let (s, e) = local_range(
        utc("2026-10-24T22:00:00Z"),
        utc("2026-10-25T00:30:00Z"),
        PARIS,
    );
    assert_eq!(
        (s, e),
        (local("2026-10-25T00:00"), local("2026-10-25T02:30"))
    );
    // Without a clock change the range is the plain local one.
    let (s, e) = local_range(
        utc("2026-12-21T13:00:00Z"),
        utc("2026-12-21T16:00:00Z"),
        PARIS,
    );
    assert_eq!(
        (s, e),
        (local("2026-12-21T14:00"), local("2026-12-21T17:00"))
    );
}

#[tokio::test]
async fn a_failed_read_blocks_the_whole_window() {
    let pool = database().await;
    pool.close().await;
    let busy = busy_intervals(
        &pool,
        "anyone",
        local("2026-12-21T00:00"),
        local("2026-12-21T23:59"),
        PARIS,
    )
    .await;
    assert_eq!(busy.len(), 1);
    assert!(busy[0].0 <= local("2026-12-21T00:00") && busy[0].1 >= local("2026-12-21T23:59"));
}

#[test]
fn a_period_across_a_fall_back_keeps_its_own_ends() {
    // Paris, 20 October 14:00 to 27 October 09:00, across 25 October.
    let (s, e) = local_range(
        utc("2026-10-20T12:00:00Z"),
        utc("2026-10-27T08:00:00Z"),
        PARIS,
    );
    assert_eq!(
        (s, e),
        (local("2026-10-20T14:00"), local("2026-10-27T09:00"))
    );
    // Santiago goes back from Saturday 24:00 to 23:00 (4 April 2026): the
    // whole Saturday stays Saturday 00:00 to Sunday 00:00.
    let santiago = chrono_tz::America::Santiago;
    let start = to_utc(santiago, local("2026-04-04T00:00"), false).unwrap();
    let end = to_utc(santiago, local("2026-04-05T00:00"), true).unwrap();
    assert_ne!(
        start.with_timezone(&santiago).offset().fix(),
        end.with_timezone(&santiago).offset().fix(),
        "the period spans the change"
    );
    let (s, e) = local_range(start, end, santiago);
    assert_eq!(
        (s, e),
        (local("2026-04-04T00:00"), local("2026-04-05T00:00"))
    );
}
