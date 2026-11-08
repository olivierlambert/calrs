//! SMS spend controls (#399): the daily cap counts billed segments, is
//! reserved atomically before each send, and meters the admin's test
//! messages too.

use super::*;

/// A webhook SMS gateway on a local port, one task per connection so
/// concurrent sends really overlap. Answers every request with `status`
/// after `delay`, and counts the requests it received. A `status` of 0
/// reads the request and hangs up without answering.
async fn spawn_gateway(
    status: u16,
    delay: std::time::Duration,
) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let received = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = received.clone();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let count = count.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf);
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text[..end]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::time::sleep(delay).await;
                if status == 0 {
                    return;
                }
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
                );
                let _ = sock.write_all(response.as_bytes()).await;
            });
        }
    });
    (format!("http://127.0.0.1:{port}/sms"), received)
}

fn webhook_config(url: &str, daily_cap: i64) -> crate::sms::SmsConfig {
    crate::sms::SmsConfig {
        provider: crate::sms::kinds::WEBHOOK.to_string(),
        sender: "calrs".to_string(),
        base_url: url.to_string(),
        default_country_code: "+33".to_string(),
        daily_cap,
        ..Default::default()
    }
}

async fn insert_usage(pool: &SqlitePool, segments: Option<i64>) {
    sqlx::query(
        "INSERT INTO sms_usage (id, event, provider, segments) VALUES (?, 'confirmed', 'webhook', ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(segments)
    .execute(pool)
    .await
    .unwrap();
}

/// A body of `chars` GSM-7 characters: one segment up to 160, two past it.
fn body_of(chars: usize) -> String {
    "a".repeat(chars)
}

#[tokio::test]
async fn the_daily_cap_counts_billed_segments() {
    let pool = setup_test_db().await;
    assert_eq!(crate::sms::segments_today(&pool).await, 0);

    insert_usage(&pool, Some(2)).await;
    insert_usage(&pool, Some(1)).await;
    // A row with no segment count (older rows, silent gateways) counts as one.
    insert_usage(&pool, None).await;
    assert_eq!(crate::sms::sent_today(&pool).await, 3);
    assert_eq!(crate::sms::segments_today(&pool).await, 4);

    // Yesterday's traffic does not count.
    sqlx::query(
        "INSERT INTO sms_usage (id, sent_at, event, provider, segments) \
             VALUES ('old', datetime('now', '-2 days'), 'reminder', 'webhook', 2)",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(crate::sms::segments_today(&pool).await, 4);
}

#[tokio::test]
async fn a_message_that_would_pass_the_segment_cap_is_not_sent() {
    let pool = setup_test_db().await;
    let (url, received) = spawn_gateway(200, std::time::Duration::ZERO).await;
    let config = webhook_config(&url, 3);
    insert_usage(&pool, Some(2)).await;

    // Two segments do not fit in the one left: refused before the gateway.
    let result =
        crate::sms::send_metered(&pool, &config, "confirmed", "+33612345678", &body_of(200)).await;
    assert!(matches!(result, Err(crate::sms::MeteredSendError::OverCap)));
    assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(crate::sms::segments_today(&pool).await, 2);

    // One segment does, and is recorded with its estimate since a webhook
    // reports no segment count.
    crate::sms::send_metered(&pool, &config, "confirmed", "+33612345678", &body_of(100))
        .await
        .expect("fits the cap");
    assert_eq!(crate::sms::segments_today(&pool).await, 3);

    // Now the cap is spent.
    let result =
        crate::sms::send_metered(&pool, &config, "reminder", "+33612345678", &body_of(10)).await;
    assert!(matches!(result, Err(crate::sms::MeteredSendError::OverCap)));
    assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 1);

    // A cap of 0 means no limit.
    crate::sms::send_metered(
        &pool,
        &webhook_config(&url, 0),
        "reminder",
        "+33612345678",
        &body_of(200),
    )
    .await
    .expect("no limit");
    assert_eq!(crate::sms::segments_today(&pool).await, 5);
}

#[tokio::test]
async fn a_refused_message_gives_its_reservation_back() {
    let pool = setup_test_db().await;
    let (url, received) = spawn_gateway(400, std::time::Duration::ZERO).await;
    let config = webhook_config(&url, 1);

    for _ in 0..2 {
        let result =
            crate::sms::send_metered(&pool, &config, "confirmed", "+33612345678", &body_of(50))
                .await;
        // A parsed 4xx is a definite refusal, not a transport failure.
        assert!(matches!(
            result,
            Err(crate::sms::MeteredSendError::Gateway(
                crate::sms::SmsError::Other(_)
            ))
        ));
    }
    // Both reached the gateway: the first refusal did not use up the cap.
    assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(crate::sms::sent_today(&pool).await, 0);
}

async fn usage_rows(pool: &SqlitePool) -> Vec<(String, Option<i64>)> {
    sqlx::query_as("SELECT status, segments FROM sms_usage ORDER BY sent_at, status")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// A request that fails in transit (a 5xx, a dropped connection) may have
/// been sent and billed anyway, so its reservation is kept, with its
/// estimate, as `unknown`, and counts against the cap.
#[tokio::test]
async fn a_send_lost_in_transit_stays_counted() {
    let pool = setup_test_db().await;
    let (url, received) = spawn_gateway(503, std::time::Duration::ZERO).await;
    let config = webhook_config(&url, 3);

    let result =
        crate::sms::send_metered(&pool, &config, "confirmed", "+33612345678", &body_of(200)).await;
    assert!(matches!(
        result,
        Err(crate::sms::MeteredSendError::Gateway(
            crate::sms::SmsError::Transport(_)
        ))
    ));
    assert_eq!(
        usage_rows(&pool).await,
        vec![("unknown".to_string(), Some(2))]
    );
    assert_eq!(crate::sms::unknown_today(&pool).await, 1);
    assert_eq!(crate::sms::segments_today(&pool).await, 2);

    // The two uncertain segments still count: two more do not fit.
    let result =
        crate::sms::send_metered(&pool, &config, "confirmed", "+33612345678", &body_of(200)).await;
    assert!(matches!(result, Err(crate::sms::MeteredSendError::OverCap)));
    assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 1);

    // A gateway that read the request and hung up without answering may
    // have sent it: kept the same way.
    let (url, received) = spawn_gateway(0, std::time::Duration::ZERO).await;
    let config = webhook_config(&url, 0);
    let result =
        crate::sms::send_metered(&pool, &config, "reminder", "+33612345678", &body_of(10)).await;
    assert!(
        matches!(
            result,
            Err(crate::sms::MeteredSendError::Gateway(
                crate::sms::SmsError::Transport(_)
            ))
        ),
        "{:?}",
        result
    );
    assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(crate::sms::unknown_today(&pool).await, 2);
    assert_eq!(crate::sms::segments_today(&pool).await, 3);
}

/// A request that never left calrs (nothing listening: connection refused)
/// sent nothing, so its reservation is given back.
#[tokio::test]
async fn a_send_that_never_left_is_released() {
    let pool = setup_test_db().await;
    let dead = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let config = webhook_config(&format!("http://127.0.0.1:{dead}/sms"), 1);
    for _ in 0..2 {
        let result =
            crate::sms::send_metered(&pool, &config, "reminder", "+33612345678", &body_of(10))
                .await;
        assert!(
            matches!(
                result,
                Err(crate::sms::MeteredSendError::Gateway(
                    crate::sms::SmsError::Unreachable(_)
                ))
            ),
            "{:?}",
            result
        );
    }
    // Both were tried (the first did not use up the cap of one), and
    // nothing is left in the ledger.
    assert_eq!(crate::sms::sent_today(&pool).await, 0);
    assert_eq!(crate::sms::unknown_today(&pool).await, 0);
}

/// An emoji in a host's title costs two UTF-16 code units, so a body the
/// character count would bill as one segment reserves two.
#[tokio::test]
async fn the_reservation_counts_utf16_code_units() {
    let pool = setup_test_db().await;
    let (url, received) = spawn_gateway(200, std::time::Duration::ZERO).await;
    let body = format!("{}🎉", "ł".repeat(69));
    assert_eq!(body.chars().count(), 70);

    let result = crate::sms::send_metered(
        &pool,
        &webhook_config(&url, 1),
        "confirmed",
        "+33612345678",
        &body,
    )
    .await;
    assert!(matches!(result, Err(crate::sms::MeteredSendError::OverCap)));
    assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 0);

    crate::sms::send_metered(
        &pool,
        &webhook_config(&url, 2),
        "confirmed",
        "+33612345678",
        &body,
    )
    .await
    .expect("fits two segments");
    // The webhook reports no count, so the estimate is what is kept.
    assert_eq!(crate::sms::segments_today(&pool).await, 2);
}

#[tokio::test]
async fn an_accepted_message_is_settled_as_sent() {
    let pool = setup_test_db().await;
    let (url, _) = spawn_gateway(200, std::time::Duration::ZERO).await;
    crate::sms::send_metered(
        &pool,
        &webhook_config(&url, 10),
        "confirmed",
        "+33612345678",
        &body_of(200),
    )
    .await
    .expect("accepted");
    assert_eq!(usage_rows(&pool).await, vec![("sent".to_string(), Some(2))]);
    assert_eq!(crate::sms::unknown_today(&pool).await, 0);
}

/// Twenty sends at once on the production pool shape, against a gateway
/// slow enough that every one of them is in flight together. Checking the
/// cap and recording after the send let all twenty through; the reservation
/// lets exactly the cap through.
#[tokio::test]
async fn concurrent_sends_never_pass_the_cap() {
    let dir = TestDataDir::new();
    let pool = crate::db::connect(&dir).await.unwrap();
    crate::db::migrate(&pool).await.unwrap();
    let (url, received) = spawn_gateway(200, std::time::Duration::from_millis(300)).await;
    let config = webhook_config(&url, 5);

    let mut tasks = Vec::new();
    for _ in 0..20 {
        let pool = pool.clone();
        let config = config.clone();
        tasks.push(tokio::spawn(async move {
            crate::sms::send_metered(&pool, &config, "confirmed", "+33612345678", &body_of(50))
                .await
        }));
    }
    let mut sent = 0;
    let mut over_cap = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(_) => sent += 1,
            Err(crate::sms::MeteredSendError::OverCap) => over_cap += 1,
            Err(e) => panic!("unexpected failure: {:?}", e),
        }
    }
    assert_eq!((sent, over_cap), (5, 15));
    assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 5);
    assert_eq!(crate::sms::segments_today(&pool).await, 5);
}

async fn configure_webhook(pool: &SqlitePool, url: &str, daily_cap: i64) {
    sqlx::query(
        "INSERT INTO sms_config (id, provider, sender, base_url, default_country_code, daily_cap) \
             VALUES ('cfg', 'webhook', 'calrs', ?, '+33', ?)",
    )
    .bind(url)
    .bind(daily_cap)
    .execute(pool)
    .await
    .unwrap();
}

fn location(response: &axum::http::Response<Body>) -> String {
    response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn the_admin_test_message_is_metered_and_capped() {
    let (app, pool, session, _) = setup_test_app().await;
    let (url, received) = spawn_gateway(200, std::time::Duration::ZERO).await;
    configure_webhook(&pool, &url, 1).await;
    let csrf = "test-csrf-sms-test";
    let body = "_csrf=test-csrf-sms-test&to=%2B33612345678";

    let response = app
        .clone()
        .oneshot(post_form("/dashboard/admin/sms/test", &session, csrf, body))
        .await
        .unwrap();
    assert!(location(&response).contains("sms_test=sent"));
    let events: Vec<String> = sqlx::query_scalar("SELECT event FROM sms_usage")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(events, vec!["test".to_string()]);
    assert_eq!(crate::sms::segments_today(&pool).await, 1);

    // The cap of one segment is spent: the next test is refused, saying why.
    let response = app
        .clone()
        .oneshot(post_form("/dashboard/admin/sms/test", &session, csrf, body))
        .await
        .unwrap();
    let to = location(&response);
    assert!(to.contains("sms_test=error"), "{}", to);
    let detail = crate::i18n::translate("en", "admin-sms-test-over-cap", None);
    assert!(to.contains(urlencoding::encode(&detail).as_ref()), "{}", to);
    assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 1);

    // Today's figures on the admin panel include the test.
    let response = app
        .clone()
        .oneshot(get_authed("/dashboard/admin", &session))
        .await
        .unwrap();
    let html = body_string(response).await;
    assert!(
        html.contains("Billed segments today:"),
        "segment figure missing"
    );
    assert!(
        !html.contains("No answer from the gateway"),
        "no unknown sends yet"
    );

    // A send whose outcome is unknown is shown, and counted.
    sqlx::query(
        "INSERT INTO sms_usage (id, event, provider, segments, status) \
             VALUES ('lost', 'confirmed', 'webhook', 2, 'unknown')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let response = app
        .clone()
        .oneshot(get_authed("/dashboard/admin", &session))
        .await
        .unwrap();
    let html = body_string(response).await;
    let figures = html
        .split("Sent today:")
        .nth(1)
        .and_then(|rest| rest.split("</div>").next())
        .unwrap_or_default()
        .to_string();
    assert!(
        figures
            .contains("No answer from the gateway, counted against the limit: <strong>1</strong>"),
        "{}",
        figures
    );
    assert!(
        figures.contains("Billed segments today: <strong>3</strong>"),
        "{}",
        figures
    );
}

#[tokio::test]
async fn the_credential_check_is_free_and_records_nothing() {
    let (app, pool, session, _) = setup_test_app().await;
    let (url, received) = spawn_gateway(200, std::time::Duration::ZERO).await;
    configure_webhook(&pool, &url, 1).await;
    let csrf = "test-csrf-sms-check";

    // An empty recipient asks for the credential check; the webhook has
    // none, so it reports that, without sending or recording anything.
    let response = app
        .clone()
        .oneshot(post_form(
            "/dashboard/admin/sms/test",
            &session,
            csrf,
            "_csrf=test-csrf-sms-check&to=",
        ))
        .await
        .unwrap();
    assert!(location(&response).contains("sms_test=error"));
    assert_eq!(received.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(crate::sms::sent_today(&pool).await, 0);
}
