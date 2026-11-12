//! Follow-ups to #422: approval and elapsed buffers across clock changes.
use super::*;

#[tokio::test]
async fn approval_checks_the_pending_requests_own_buffers() {
    // Both approval entry points share the same check. Try a conflict on
    // either side, then end it and verify a real approval still succeeds.
    for dashboard in [false, true] {
        for before in [false, true] {
            let (app, pool, session, et) = setup_test_app().await;
            sqlx::query("UPDATE event_types SET buffer_before = ?, buffer_after = ?, pending_blocks_slot = 0, requires_confirmation = 1 WHERE id = ?")
                .bind(if before { 30 } else { 0 }).bind(if before { 0 } else { 30 }).bind(&et).execute(&pool).await.unwrap();
            let host: String =
                sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            let other = insert_personal_et(&pool, &host, "other-unbuffered").await;
            sqlx::query("UPDATE event_types SET buffer_before = 0, buffer_after = 0 WHERE id = ?")
                .bind(&other)
                .execute(&pool)
                .await
                .unwrap();
            let day = NaiveDate::parse_from_str(&far_future_monday(), "%Y-%m-%d").unwrap();
            let ten = day.and_hms_opt(10, 0, 0).unwrap().and_utc();
            let pending =
                insert_v1_booking(&pool, &et, "pending@example.com", ten, 30, "pending", &[]).await;
            let conflict = insert_v1_booking(
                &pool,
                &other,
                "confirmed@example.com",
                ten + Duration::minutes(if before { -30 } else { 30 }),
                30,
                "confirmed",
                &[],
            )
            .await;
            let request = || {
                if dashboard {
                    post_form(
                        &format!("/dashboard/bookings/{pending}/confirm"),
                        &session,
                        "buf",
                        "_csrf=buf",
                    )
                } else {
                    post_bare(&format!("/booking/approve/confirm-{pending}"))
                }
            };
            let response = app.clone().oneshot(request()).await.unwrap();
            assert!(!response.status().is_server_error());
            assert_eq!(
                booking_status(&pool, &pending).await,
                "pending",
                "approval must keep its own buffer, dashboard={dashboard}, before={before}"
            );
            sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
                .bind(&conflict)
                .execute(&pool)
                .await
                .unwrap();
            let response = app.oneshot(request()).await.unwrap();
            assert!(!response.status().is_server_error());
            assert_eq!(
                booking_status(&pool, &pending).await,
                "confirmed",
                "approval should work once the buffered interval is free"
            );
        }
    }
}

/// A spring clock change that skips 02:00 to 03:00 local time, for the
/// HTTP tests: its midnight still at least an hour ahead, and its day within
/// the booking date cap (366 days), whenever the tests run. Paris (last
/// Sunday of March) and Sydney (first Sunday of October) skip the same local
/// hour about half a year apart, so one of them always fits; Paris alone can
/// be 371 days away.
fn next_spring_change() -> (NaiveDate, &'static str) {
    let now = Utc::now();
    let today = now.date_naive();
    let mut candidates = Vec::new();
    for year in today.year()..=today.year() + 1 {
        let mut paris = NaiveDate::from_ymd_opt(year, 3, 31).unwrap();
        while paris.weekday() != chrono::Weekday::Sun {
            paris -= Duration::days(1);
        }
        candidates.push((paris, "Europe/Paris"));
        let mut sydney = NaiveDate::from_ymd_opt(year, 10, 1).unwrap();
        while sydney.weekday() != chrono::Weekday::Sun {
            sydney += Duration::days(1);
        }
        candidates.push((sydney, "Australia/Sydney"));
    }
    candidates.sort();
    candidates
        .into_iter()
        .find(|(day, zone)| {
            let tz: chrono_tz::Tz = zone.parse().unwrap();
            let midnight = tz
                .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
                .single()
                .unwrap();
            midnight > now + Duration::hours(1) && *day <= today + Duration::days(365)
        })
        .expect("a spring change every half year")
}

#[test]
fn the_chosen_spring_change_skips_two_to_three() {
    let (day, zone) = next_spring_change();
    let tz: chrono_tz::Tz = zone.parse().unwrap();
    for (time, exists) in [("01:59", true), ("02:30", false), ("03:00", true)] {
        let t = day.and_time(chrono::NaiveTime::parse_from_str(time, "%H:%M").unwrap());
        assert_eq!(
            tz.from_local_datetime(&t).single().is_some(),
            exists,
            "{zone} {day} {time}"
        );
    }
}

#[tokio::test]
async fn booking_buffers_keep_elapsed_minutes_in_both_booking_orders() {
    for before in [false, true] {
        for reversed in [false, true] {
            let (app, pool, _, et) = setup_test_app().await;
            let (day, zone) = next_spring_change();
            let day = day.to_string();
            sqlx::query("UPDATE event_types SET timezone = ?, buffer_before = ?, buffer_after = ? WHERE id = ?")
                .bind(zone).bind(if before { 120 } else { 0 }).bind(if before { 0 } else { 120 }).bind(&et).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES ('dst-sunday', ?, 0, '00:00', '08:00')").bind(&et).execute(&pool).await.unwrap();
            let (first, refused, allowed) = if reversed {
                ("03:30", "01:00", "00:00")
            } else {
                ("01:00", "03:30", "04:30")
            };
            for (time, expected_count) in [(first, 1), (refused, 1), (allowed, 2)] {
                let response = app.clone().oneshot(post_form_unauthed("/u/testuser/test-meeting/book", "buf", &format!("_csrf=buf&date={day}&time={time}&tz={}&name=Guest&email=guest%40example.com", zone.replace('/', "%2F")))).await.unwrap();
                assert!(!response.status().is_server_error());
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
                assert_eq!(
                    count, expected_count,
                    "before={before}, reversed={reversed}, time={time}"
                );
            }
            let rows: Vec<(String, String)> =
                sqlx::query_as("SELECT start_at, end_at FROM bookings ORDER BY start_at")
                    .fetch_all(&pool)
                    .await
                    .unwrap();
            let end = chrono::DateTime::parse_from_rfc3339(&rows[0].1).unwrap();
            let next = chrono::DateTime::parse_from_rfc3339(&rows[1].0).unwrap();
            assert_eq!(
                (next - end).num_minutes(),
                120,
                "the exact boundary remains bookable"
            );
        }
    }
}
