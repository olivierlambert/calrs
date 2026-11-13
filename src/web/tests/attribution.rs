//! Campaign tracking: the `utm_*` parameters travel from the host's page to
//! the booking and are stored with it.

use super::*;

const UTM: &str = "utm_source=website&utm_campaign=pricing";

async fn page(app: &Router, uri: &str) -> String {
    body_string(app.clone().oneshot(get(uri)).await.unwrap()).await
}

#[tokio::test]
async fn utm_parameters_follow_the_visitor_to_the_booking() {
    let (app, pool, _, et_id) = setup_test_app().await;

    // Every page on the way keeps them in its links, nothing else.
    let profile = page(&app, &format!("/u/testuser?{UTM}&tz=UTC")).await;
    assert!(
        profile.contains("/u/testuser/test-meeting?utm_campaign=pricing&amp;utm_source=website"),
        "{profile}"
    );
    let slots = page(&app, &format!("/u/testuser/test-meeting?{UTM}")).await;
    assert!(
        slots.contains("decHtml(\"&utm_campaign=pricing&amp;utm_source=website\")"),
        "{slots}"
    );
    let form = page(&app, &format!("{}&{UTM}", booking_form_uri(None))).await;
    assert!(
        form.contains(r#"<input type="hidden" name="utm_source" value="website">"#),
        "{form}"
    );
    assert!(form.contains(r#"<input type="hidden" name="utm_campaign" value="pricing">"#));
    assert!(
        !form.contains(r#"name="utm_medium""#),
        "unset values are not posted"
    );

    // The booking stores what the form posted back.
    let csrf = "utm";
    let body = format!("{}&{UTM}", book_body(csrf));
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &body,
        ))
        .await
        .unwrap();
    let confirmation = body_string(response).await;
    let stored: (String, Option<String>, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT b.id, bt.utm_source, bt.utm_medium, bt.utm_campaign
         FROM bookings b JOIN booking_attribution bt ON bt.booking_id = b.id
         WHERE b.event_type_id = ?",
    )
    .bind(&et_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let (_, source, medium, campaign) = stored;
    assert_eq!(
        (source.as_deref(), medium, campaign.as_deref()),
        (Some("website"), None, Some("pricing"))
    );
    assert!(
        !confirmation.contains("pricing"),
        "the guest's page never shows it"
    );
}

#[tokio::test]
async fn a_booking_without_utm_parameters_stores_nothing() {
    let (app, pool, _, _) = setup_test_app().await;
    let csrf = "no-utm";
    app.clone()
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &format!("{}&utm_source=%20%20", book_body(csrf)),
        ))
        .await
        .unwrap();
    let bookings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bookings")
        .fetch_one(&pool)
        .await
        .unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM booking_attribution")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((bookings, rows), (1, 0));
}

/// What guests, attendees, calendars and SMS gateways receive is built
/// without ever reading the attribution, so no later change to those paths
/// can leak it by reusing a query.
#[test]
fn guest_facing_code_never_reads_attribution() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<_> = [
        "src/web/calendar_push.rs",
        "src/web/writeback.rs",
        "src/web/meeting.rs",
        "src/google_meet.rs",
        "src/resources.rs",
    ]
    .into_iter()
    .map(|f| root.join(f))
    .collect();
    for dir in [
        "src/email",
        "src/sms",
        "src/providers",
        "src/caldav",
        "src/ews",
    ] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    for path in files {
        let src = std::fs::read_to_string(&path).unwrap();
        assert!(
            !src.contains("attribution") && !src.contains("utm_"),
            "{} reads the booking attribution",
            path.display()
        );
    }
    for template in [
        "confirmed.html",
        "booking_cancel_form.html",
        "booking_cancelled_guest.html",
        "booking_reschedule_confirm.html",
        "booking_claim_form.html",
        "booking_claimed.html",
        "booking_approve_form.html",
        "booking_decline_form.html",
    ] {
        let src = std::fs::read_to_string(root.join("templates").join(template)).unwrap();
        assert!(
            !src.contains("utm"),
            "templates/{template} shows the booking attribution"
        );
    }
}

/// A private team's page joins its invite and the visitor's parameters in
/// one query, and renders at all (`urlencode` needs minijinja's feature).
#[tokio::test]
async fn a_private_team_page_keeps_both_the_invite_and_the_utm_parameters() {
    let (app, pool, _, _) = setup_test_app().await;
    let owner: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (team_id, _) = insert_team_with_et(&pool, "utm-team", "rr", &[(&owner, "admin")]).await;
    sqlx::query("UPDATE teams SET visibility = 'private', invite_token = 'tok' WHERE id = ?")
        .bind(&team_id)
        .execute(&pool)
        .await
        .unwrap();
    let html = page(&app, "/team/utm-team?invite=tok&utm_source=website").await;
    assert!(
        html.contains("/team/utm-team/rr?invite=tok&amp;utm_source=website"),
        "{html}"
    );
}
