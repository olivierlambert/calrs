//! Tests for the web module. Shared fixtures and router helpers live
//! here; each child module covers one area and reaches them, and every
//! item of `web`, through `use super::*`.

use super::*;

mod admin;
mod auth_pages;
mod availability;
mod booking;
mod booking_actions;
mod booking_time;
mod branding;
mod dashboard;
mod emails;
mod event_types;
mod format;
mod guards;
mod hosts;
mod invites;
mod mcp;
mod oauth;
mod organizer;
mod page_language;
mod pending_hold;
mod public_pages;
mod resources;
mod security;
mod sources;
mod writeback_failures;

// --- has_conflict tests ---

fn dt(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(y, m, d)
        .unwrap()
        .and_hms_opt(h, mi, 0)
        .unwrap()
}

// --- Integration tests with in-memory SQLite ---

async fn setup_test_db() -> SqlitePool {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
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

/// Insert test fixtures: user, account, event type, availability rules
async fn seed_test_data(pool: &SqlitePool) -> (String, String, String) {
    let user_id = uuid::Uuid::new_v4().to_string();
    let account_id = uuid::Uuid::new_v4().to_string();
    let et_id = uuid::Uuid::new_v4().to_string();

    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'test@example.com', 'Test User', 'admin', 'local', 'testuser', 1)")
        .bind(&user_id)
        .execute(pool).await.unwrap();

    sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Test User', 'test@example.com', 'UTC', ?)")
        .bind(&account_id)
        .bind(&user_id)
        .execute(pool).await.unwrap();

    sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) VALUES (?, ?, 'test-meeting', 'Test Meeting', 30, 0, 0, 0, 1)")
        .bind(&et_id)
        .bind(&account_id)
        .execute(pool).await.unwrap();

    // Mon-Fri 09:00-17:00
    for day in [1, 2, 3, 4, 5] {
        let rule_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '09:00', '17:00')")
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .execute(pool).await.unwrap();
    }

    (user_id, account_id, et_id)
}

// --- Authorization helpers: can_manage_event_type / find_manageable_event_type_by_slug ---

/// Insert a user with a configurable role. Returns user_id.
async fn insert_role_user(pool: &SqlitePool, email: &str, role: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let username = email.split('@').next().unwrap_or("u").to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled) \
             VALUES (?, ?, ?, ?, 'local', ?, 1)",
    )
    .bind(&id)
    .bind(email)
    .bind(email)
    .bind(role)
    .bind(&username)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Insert a personal event type owned by `user_id`. Creates the account row
/// if missing. Returns event_type_id.
async fn insert_personal_et(pool: &SqlitePool, user_id: &str, slug: &str) -> String {
    let account_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .unwrap();
    let account_id = match account_id {
        Some(id) => id,
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO accounts (id, name, email, timezone, user_id) \
                     VALUES (?, 'A', 'a@x', 'UTC', ?)",
            )
            .bind(&id)
            .bind(user_id)
            .execute(pool)
            .await
            .unwrap();
            id
        }
    };
    let et_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) \
             VALUES (?, ?, ?, 'P', 30, 0, 0, 0, 1)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(slug)
    .execute(pool)
    .await
    .unwrap();
    et_id
}

/// Insert a team and a team event type. `members` is a list of (user_id, role).
/// Returns (team_id, event_type_id).
async fn insert_team_with_et(
    pool: &SqlitePool,
    team_slug: &str,
    et_slug: &str,
    members: &[(&str, &str)],
) -> (String, String) {
    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO teams (id, name, slug, visibility) VALUES (?, ?, ?, 'public')")
        .bind(&team_id)
        .bind(team_slug)
        .bind(team_slug)
        .execute(pool)
        .await
        .unwrap();
    for (uid, role) in members {
        sqlx::query(
            "INSERT INTO team_members (team_id, user_id, role, source) \
                 VALUES (?, ?, ?, 'direct')",
        )
        .bind(&team_id)
        .bind(uid)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
    }
    // Each team event gets its own placeholder account to avoid colliding
    // with personal events under the unique(account_id, slug) constraint.
    let account_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO accounts (id, name, email, timezone) VALUES (?, 't', 't@x', 'UTC')")
        .bind(&account_id)
        .execute(pool)
        .await
        .unwrap();
    let et_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO event_types (id, account_id, team_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) \
             VALUES (?, ?, ?, ?, 'T', 30, 0, 0, 0, 1)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(&team_id)
    .bind(et_slug)
    .execute(pool)
    .await
    .unwrap();
    (team_id, et_id)
}

/// Seed a CalDAV source + busy calendar + one synced event under `account_id`.
/// `start_at`/`end_at` are compact iCal strings (e.g. "20260618T100000").
async fn seed_synced_event(
    pool: &SqlitePool,
    account_id: &str,
    start_at: &str,
    end_at: &str,
    timezone: &str,
    transp: &str,
) {
    let source_id = uuid::Uuid::new_v4().to_string();
    let cal_id = uuid::Uuid::new_v4().to_string();
    let event_id = uuid::Uuid::new_v4().to_string();
    let uid = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO caldav_sources (id, account_id, name, url, username) VALUES (?, ?, 'Src', 'https://x/dav', 'u')")
        .bind(&source_id).bind(account_id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?, ?, '/cal/', 'Cal', 1)")
        .bind(&cal_id).bind(&source_id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, all_day, timezone, status, transp) VALUES (?, ?, ?, 'Busy', ?, ?, 0, ?, 'CONFIRMED', ?)")
        .bind(&event_id).bind(&cal_id).bind(&uid).bind(start_at).bind(end_at).bind(timezone).bind(transp)
        .execute(pool).await.unwrap();
}

async fn insert_booking(
    pool: &SqlitePool,
    et_id: &str,
    assigned_user_id: Option<&str>,
    start: NaiveDateTime,
    status: &str,
) {
    let end = start + Duration::minutes(30);
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id) VALUES (?, ?, ?, 'G', 'g@e.com', 'UTC', ?, ?, ?, ?, ?, ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(start.format("%Y-%m-%dT%H:%M:%S").to_string())
        .bind(end.format("%Y-%m-%dT%H:%M:%S").to_string())
        .bind(status)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(assigned_user_id)
        .execute(pool).await.unwrap();
}

// --- Shared bookable resources tests ---

/// Insert a resource with a fresh last_synced_at so sync_if_stale never
/// tries to fetch the feed over the network during tests.
async fn insert_resource(pool: &SqlitePool, name: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO resources (id, name, feed_url, last_synced_at) \
             VALUES (?, ?, 'https://feed.invalid/cal.ics', datetime('now'))",
    )
    .bind(&id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn attach_resource(pool: &SqlitePool, et_id: &str, resource_id: &str) {
    sqlx::query("INSERT INTO event_type_resources (event_type_id, resource_id) VALUES (?, ?)")
        .bind(et_id)
        .bind(resource_id)
        .execute(pool)
        .await
        .unwrap();
}

// --- Reschedule DB flow tests ---

/// Helper to insert a confirmed booking and return (booking_id, reschedule_token, cancel_token)
async fn insert_test_booking(
    pool: &SqlitePool,
    et_id: &str,
    start: &str,
    end: &str,
    status: &str,
) -> (String, String, String) {
    let booking_id = uuid::Uuid::new_v4().to_string();
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let confirm_token = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token) VALUES (?, ?, ?, 'Guest', 'guest@example.com', 'UTC', ?, ?, ?, ?, ?, ?)"
    )
    .bind(&booking_id)
    .bind(et_id)
    .bind(format!("{}@calrs", uuid::Uuid::new_v4()))
    .bind(start)
    .bind(end)
    .bind(status)
    .bind(&cancel_token)
    .bind(&reschedule_token)
    .bind(&confirm_token)
    .execute(pool).await.unwrap();
    (booking_id, reschedule_token, cancel_token)
}

// --- HTTP integration tests ---

use axum::body::Body;
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn setup_test_app() -> (Router, SqlitePool, String, String) {
    let pool = setup_test_db().await;
    let (user_id, _account_id, et_id) = seed_test_data(&pool).await;

    // Create a session for the test user
    let session_token = uuid::Uuid::new_v4().to_string();
    let expires_at = (Utc::now() + Duration::days(30))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&session_token)
        .bind(&user_id)
        .bind(&expires_at)
        .execute(&pool)
        .await
        .unwrap();

    let data_dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();

    let router = create_router(pool.clone(), data_dir, [0u8; 32]).await;
    (router, pool, session_token, et_id)
}

async fn insert_test_user_with_session(
    pool: &SqlitePool,
    name: &str,
    email: &str,
    username: &str,
) -> (String, String) {
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, name, role, auth_provider, username, enabled) \
             VALUES (?, ?, ?, 'user', 'local', ?, 1)",
    )
    .bind(&user_id)
    .bind(email)
    .bind(name)
    .bind(username)
    .execute(pool)
    .await
    .unwrap();

    let session_token = uuid::Uuid::new_v4().to_string();
    let expires_at = (Utc::now() + Duration::days(30))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&session_token)
        .bind(&user_id)
        .bind(&expires_at)
        .execute(pool)
        .await
        .unwrap();

    (user_id, session_token)
}

fn get(uri: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn get_authed(uri: &str, session: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .uri(uri)
        .header("cookie", format!("__Host-calrs_session={}", session))
        .body(Body::empty())
        .unwrap()
}

fn booking_form_uri(invite: Option<&str>) -> String {
    let date = (Utc::now() + Duration::days(1)).format("%Y-%m-%d");
    let invite = invite
        .map(|token| format!("&invite={token}"))
        .unwrap_or_default();
    format!("/u/testuser/test-meeting/book?date={date}&time=10:00{invite}")
}

async fn body_string(response: axum::http::Response<Body>) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Values of the selected options in the `<select id="{id}">` of a page,
/// unescaped, so a test can submit what a browser would.
fn selected_options(body: &str, id: &str) -> Vec<String> {
    let start = body
        .find(&format!(r#"<select id="{id}""#))
        .unwrap_or_else(|| panic!("no <select id={id:?}>"));
    let end = start + body[start..].find("</select>").unwrap();
    body[start..end]
        .split("<option ")
        .skip(1)
        .filter(|opt| opt.split('>').next().unwrap_or("").contains(" selected"))
        .map(|opt| {
            let value = opt.split('"').nth(1).unwrap_or_default();
            value.replace("&#x2f;", "/").replace("&#x2b;", "+")
        })
        .collect()
}

// --- POST handler helpers ---

fn post_form(uri: &str, session: &str, csrf: &str, body: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "cookie",
            format!(
                "__Host-calrs_session={}; __Host-calrs_csrf={}",
                session, csrf
            ),
        )
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn post_form_unauthed(uri: &str, csrf: &str, body: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .header("cookie", format!("__Host-calrs_csrf={}", csrf))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn post_bare(uri: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::empty())
        .unwrap()
}

// --- Min-notice for guest cancel / reschedule ---

/// Format a `start_at` value `offset` from now, using the same shape
/// (`%Y-%m-%dT%H:%M:%S`) that booking handlers persist. Booking times
/// are stored as naive host-local timestamps; the notice check uses
/// `Local::now().naive_local()` so we mirror that here.
fn booking_start_at(offset: chrono::Duration) -> String {
    (Local::now().naive_local() + offset)
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

fn booking_end_at(offset: chrono::Duration, duration_min: i64) -> String {
    (Local::now().naive_local() + offset + chrono::Duration::minutes(duration_min))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

async fn insert_booking_at(
    pool: &SqlitePool,
    et_id: &str,
    cancel_tok: &str,
    resched_tok: &str,
    offset: chrono::Duration,
) -> String {
    let booking_id = uuid::Uuid::new_v4().to_string();
    let start_at = booking_start_at(offset);
    let end_at = booking_end_at(offset, 30);
    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid-notice', 'Guest', 'guest@test.com', 'UTC', ?, ?, 'confirmed', ?, ?)",
    )
    .bind(&booking_id)
    .bind(et_id)
    .bind(&start_at)
    .bind(&end_at)
    .bind(cancel_tok)
    .bind(resched_tok)
    .execute(pool)
    .await
    .unwrap();
    booking_id
}

fn host_today() -> NaiveDate {
    Utc::now().with_timezone(&Tz::UTC).naive_local().date()
}

/// A Monday roughly a month out: past any small horizon, and on a weekday
/// the seeded Mon-Fri availability covers.
fn far_future_monday() -> String {
    let mut d = host_today() + Duration::days(28);
    while d.weekday() != chrono::Weekday::Mon {
        d += Duration::days(1);
    }
    d.format("%Y-%m-%d").to_string()
}

fn book_body(csrf: &str) -> String {
    format!(
        "_csrf={}&date={}&time=10:00&name=Guest&email=guest%40test.com&tz=UTC",
        csrf,
        far_future_monday()
    )
}

async fn set_horizon(pool: &SqlitePool, et_id: &str, days: i32) {
    sqlx::query("UPDATE event_types SET booking_horizon_days = ? WHERE id = ?")
        .bind(days)
        .bind(et_id)
        .execute(pool)
        .await
        .unwrap();
}

/// Seed a team plus a team event type sharing the test account, with the
/// same Mon-Fri availability as the personal fixture.
async fn seed_team_event_type(pool: &SqlitePool) -> (String, String) {
    let (user_id, account_id): (String, String) = sqlx::query_as(
        "SELECT u.id, a.id FROM users u JOIN accounts a ON a.user_id = u.id WHERE u.username = 'testuser'",
    )
    .fetch_one(pool)
    .await
    .unwrap();

    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO teams (id, name, slug, visibility, created_by) VALUES (?, 'Horizon Team', 'horizon-team', 'public', ?)")
        .bind(&team_id)
        .bind(&user_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')")
        .bind(&team_id)
        .bind(&user_id)
        .execute(pool)
        .await
        .unwrap();

    let et_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, team_id, created_by_user_id) \
             VALUES (?, ?, 'team-sync', 'Team Sync', 30, 0, 0, 0, 1, ?, ?)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(&team_id)
    .bind(&user_id)
    .execute(pool)
    .await
    .unwrap();

    for day in [1, 2, 3, 4, 5] {
        sqlx::query("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '09:00', '17:00')")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&et_id)
            .bind(day)
            .execute(pool)
            .await
            .unwrap();
    }

    (team_id, et_id)
}

// --- #209: guest emails sent on someone else's request keep the guest's language ---

/// One message as an anonymous loopback MTA received it.
struct CapturedMail {
    rcpt: Vec<String>,
    data: String,
}

/// Anonymous loopback MTA (no AUTH, no STARTTLS) that accepts any number
/// of connections and keeps every message, so a handler test can read
/// the emails it sent. calrs opens one connection per email.
async fn spawn_capturing_mta() -> (u16, Arc<std::sync::Mutex<Vec<CapturedMail>>>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mails = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = mails.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let sink = sink.clone();
            tokio::spawn(async move {
                let (read_half, mut write) = stream.into_split();
                let mut reader = BufReader::new(read_half);
                let _ = write.write_all(b"220 fake.local ESMTP\r\n").await;
                let mut rcpt = Vec::new();
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    let upper = line.trim_end().to_ascii_uppercase();
                    let reply: &[u8] = if upper.starts_with("EHLO") {
                        b"250-fake.local\r\n250 8BITMIME\r\n"
                    } else if upper.starts_with("RCPT TO") {
                        rcpt.push(line.trim_end().to_string());
                        b"250 OK\r\n"
                    } else if upper == "DATA" {
                        let _ = write.write_all(b"354 go ahead\r\n").await;
                        let mut data = String::new();
                        loop {
                            let mut body = String::new();
                            match reader.read_line(&mut body).await {
                                Ok(0) | Err(_) => break,
                                Ok(_) => {}
                            }
                            if body.trim_end() == "." {
                                break;
                            }
                            data.push_str(&body);
                        }
                        sink.lock().unwrap().push(CapturedMail {
                            rcpt: std::mem::take(&mut rcpt),
                            data,
                        });
                        b"250 queued\r\n"
                    } else if upper == "QUIT" {
                        let _ = write.write_all(b"221 bye\r\n").await;
                        break;
                    } else {
                        b"250 OK\r\n"
                    };
                    let _ = write.write_all(reply).await;
                }
            });
        }
    });
    (port, mails)
}

/// Point the instance's SMTP config at the capturing MTA.
async fn use_capturing_mta(pool: &SqlitePool, port: u16) {
    let password_enc = crate::crypto::encrypt_password(&[0u8; 32], "").unwrap();
    sqlx::query(
        "INSERT INTO smtp_config (id, host, port, username, password_enc, from_email, tls_mode, enabled) \
             VALUES (?, '127.0.0.1', ?, '', ?, 'noreply@test.com', 'none', 1)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(port as i64)
    .bind(&password_enc)
    .execute(pool)
    .await
    .unwrap();
}

/// Undo quoted-printable transfer encoding so assertions can match the
/// text the recipient reads, accents included.
fn decode_quoted_printable(raw: &str) -> String {
    let joined = raw.replace("=\r\n", "").replace("=\n", "");
    let bytes = joined.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&joined[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The decoded body of the one email addressed to `email`. Booking emails
/// are sent from a background task (#236), so wait for it to arrive.
async fn mail_to(mails: &std::sync::Mutex<Vec<CapturedMail>>, email: &str) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        {
            let mails = mails.lock().unwrap();
            let found: Vec<&CapturedMail> = mails
                .iter()
                .filter(|m| m.rcpt.iter().any(|r| r.contains(email)))
                .collect();
            if !found.is_empty() || std::time::Instant::now() >= deadline {
                assert_eq!(found.len(), 1, "expected one email to {email}");
                return decode_quoted_printable(&found[0].data);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Every email to `email` once the background sends have finished, decoded.
/// Unlike `mail_to`, which returns at the first one, this sees a second
/// email that a test must rule out.
async fn settled_mails_to(mails: &std::sync::Mutex<Vec<CapturedMail>>, email: &str) -> Vec<String> {
    drain_emails(std::time::Duration::from_secs(10)).await;
    mails
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m.rcpt.iter().any(|r| r.contains(email)))
        .map(|m| decode_quoted_printable(&m.data))
        .collect()
}

async fn insert_pending_french_booking(pool: &SqlitePool, et_id: &str, uid: &str) -> String {
    let booking_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, language) VALUES (?, ?, ?, 'Guest', 'guest@test.com', 'UTC', '2026-06-15T10:00:00', '2026-06-15T10:30:00', 'pending', ?, ?, ?, 'fr')")
        .bind(&booking_id)
        .bind(et_id)
        .bind(uid)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(format!("confirm-{uid}"))
        .execute(pool)
        .await
        .unwrap();
    booking_id
}

// --- #221: guest emails come from the host, with Reply-To ---

/// The value of the first `name:` header of a captured message.
fn mail_header<'a>(mail: &'a str, name: &str) -> Option<&'a str> {
    let headers = mail.split("\r\n\r\n").next().unwrap_or(mail);
    let prefix = format!("{name}: ");
    headers
        .split("\r\n")
        .find_map(|line| line.strip_prefix(prefix.as_str()))
}

// --- #209: host, member and watcher emails follow each recipient's language ---

async fn set_user_language(pool: &SqlitePool, email: &str, language: &str) {
    sqlx::query("UPDATE users SET language = ? WHERE email = ?")
        .bind(language)
        .bind(email)
        .execute(pool)
        .await
        .unwrap();
}

fn get_with_language(uri: &str, accept: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .uri(uri)
        .header("accept-language", accept)
        .body(Body::empty())
        .unwrap()
}

/// Give `user_id` a basic-auth CalDAV source on `base` that writes to
/// `/dav/cal/`.
async fn seed_write_source(pool: &SqlitePool, user_id: &str, base: &str) -> String {
    let account_id: String = sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .unwrap();
    let source_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, enabled, write_calendar_href) \
         VALUES (?, ?, 'Work calendar', ?, 'u', ?, 1, '/dav/cal/')",
    )
    .bind(&source_id)
    .bind(&account_id)
    .bind(base)
    .bind(crate::crypto::encrypt_password(&[0u8; 32], "p").unwrap())
    .execute(pool)
    .await
    .unwrap();
    source_id
}

/// A UTC instant as stored on a version 1 booking.
fn utc_stamp(t: chrono::DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Insert a version 1 (UTC) booking and return its id. `extra` is a list of
/// (column, value) pairs set right after the insert.
async fn insert_v1_booking(
    pool: &SqlitePool,
    et_id: &str,
    email: &str,
    start: chrono::DateTime<Utc>,
    minutes: i64,
    status: &str,
    extra: &[(&str, Option<String>)],
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO bookings (time_version, id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token)
         VALUES (1, ?, ?, ?, 'Guest', ?, 'UTC', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(et_id)
    .bind(format!("uid-{id}"))
    .bind(email)
    .bind(utc_stamp(start))
    .bind(utc_stamp(start + Duration::minutes(minutes)))
    .bind(status)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(format!("confirm-{id}"))
    .execute(pool)
    .await
    .unwrap();
    for (col, value) in extra {
        sqlx::query(&format!("UPDATE bookings SET {col} = ? WHERE id = ?"))
            .bind(value)
            .bind(&id)
            .execute(pool)
            .await
            .unwrap();
    }
    id
}

async fn booking_status(pool: &SqlitePool, id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM bookings WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}
