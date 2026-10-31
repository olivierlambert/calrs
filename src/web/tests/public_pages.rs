//! Public profile, slot and booking form pages, and their templates.

use super::availability::horizon_window;
use super::*;

// --- Template rendering regression tests ---

/// Maximum `<a>` nesting depth in rendered HTML. HTML parsers close an
/// open anchor when they meet another `<a>`, so anything above 1 means the
/// browser will tear the outer element apart.
fn max_anchor_depth(html: &str) -> usize {
    let lower = html.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let (mut depth, mut max, mut i) = (0usize, 0usize, 0usize);
    while i < bytes.len() {
        if bytes[i..].starts_with(b"</a>") {
            depth = depth.saturating_sub(1);
            i += 4;
        } else if bytes[i..].starts_with(b"<a") && matches!(bytes.get(i + 2), Some(b' ' | b'>')) {
            depth += 1;
            max = max.max(depth);
            i += 2;
        } else {
            i += 1;
        }
    }
    max
}

/// Regression test for #239: an event-type description containing a
/// markdown link must not produce an `<a>` nested inside the card link on
/// the user and team profile pages, and the description link must survive.
#[test]
fn profile_cards_do_not_nest_description_links() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);

    let description =
        crate::utils::render_inline_markdown("See [the agenda](https://example.com/agenda)");
    assert!(description.contains("<a "), "markdown should render a link");
    let event_types = vec![context! {
        slug => "intro",
        title => "Intro Call",
        duration_min => 30,
        description => description,
        visibility => "public",
    }];

    let profile = env
        .get_template("profile.html")
        .expect("profile.html should load")
        .render(context! {
            host_name => "Alice",
            username => "alice",
            event_types => event_types.clone(),
            lang => "en",
        })
        .expect("profile.html should render");
    let team = env
        .get_template("team_profile.html")
        .expect("team_profile.html should load")
        .render(context! {
            team_name => "Sales",
            team_slug => "sales",
            event_types => event_types,
            lang => "en",
        })
        .expect("team_profile.html should render");

    for (name, html, href) in [
        ("profile.html", &profile, "href=\"/u/alice/intro\""),
        ("team_profile.html", &team, "href=\"/team/sales/intro\""),
    ] {
        assert_eq!(max_anchor_depth(html), 1, "{name} nests an <a> in an <a>");
        assert!(
            html.contains("https://example.com/agenda"),
            "{name} must keep the description link"
        );
        assert!(html.contains(href), "{name} must keep the card link");
    }
}

/// Render slots.html WITHOUT reschedule context and verify slot links
/// point to /book and not to a JSON object.
/// Regression test for: default(value='') rendering as {"value": ""}
#[test]
fn slots_template_links_without_reschedule_context() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);

    let tmpl = env
        .get_template("slots.html")
        .expect("slots.html should load");

    // Minimal context mimicking show_slots_for_user (no reschedule_base)
    let rendered = tmpl
        .render(context! {
            event_type => context! {
                slug => "intro",
                title => "Intro Call",
                duration_min => 30,
            },
            host_name => "Alice",
            username => "alice",
            days => vec![
                context! {
                    date => "2026-03-16",
                    label => "Mon",
                    slots => vec![
                        context! { start => "10:00", end => "10:30", host_date => "2026-03-16", host_time => "10:00", guest_date => "2026-03-16" },
                    ],
                },
            ],
            available_dates => vec!["2026-03-16"],
            month_label => "March 2026",
            month_year => "2026-03",
            next_month => "2026-04",
            first_weekday => 0,
            days_in_month => 31,
            today_date => "2026-03-14",
            guest_tz => "UTC",
        })
        .expect("slots.html should render");

    // basePath must be /u/alice/intro, not something with { in it
    assert!(
        rendered.contains("&#x2f;u&#x2f;alice&#x2f;intro") || rendered.contains("/u/alice/intro"),
        "basePath should be /u/alice/intro"
    );

    // rescheduleBase must be empty, not a JSON object
    assert!(
        !rendered.contains(r#"{"value"#),
        "rescheduleBase must not render as a JSON object"
    );

    // Slot links should go to /book, not to a bare {
    assert!(
        !rendered.contains(r#"href="{"#),
        "Slot hrefs must not start with opening brace"
    );
}

/// Render slots.html WITH reschedule context and verify slot links
/// use the reschedule URL, not the /book path.
#[test]
fn slots_template_links_with_reschedule_context() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);

    let tmpl = env
        .get_template("slots.html")
        .expect("slots.html should load");

    let rendered = tmpl
        .render(context! {
            event_type => context! {
                slug => "intro",
                title => "Intro Call",
                duration_min => 30,
            },
            host_name => "Alice",
            username => "alice",
            reschedule_base => "/booking/reschedule/abc123",
            reschedule_info => context! {
                event_title => "Intro Call",
                old_date => "2026-03-15",
                old_time => "10:00",
            },
            days => vec![
                context! {
                    date => "2026-03-16",
                    label => "Mon",
                    slots => vec![
                        context! { start => "10:00", end => "10:30", host_date => "2026-03-16", host_time => "10:00", guest_date => "2026-03-16" },
                    ],
                },
            ],
            available_dates => vec!["2026-03-16"],
            month_label => "March 2026",
            month_year => "2026-03",
            next_month => "2026-04",
            first_weekday => 0,
            days_in_month => 31,
            today_date => "2026-03-14",
            guest_tz => "UTC",
        })
        .expect("slots.html should render");

    // basePath should be the reschedule URL, not the normal /u/alice/intro
    assert!(
        rendered.contains("reschedule&#x2f;abc123") || rendered.contains("reschedule/abc123"),
        "basePath should be the reschedule URL when reschedule_base is set"
    );

    // Reschedule banner should appear
    assert!(
        rendered.contains("Rescheduling:"),
        "Reschedule banner should be visible"
    );

    // Banner must be OUTSIDE slots-outer (not a flex child of the 3-column layout).
    // Search in the HTML body, skipping the <style> block where CSS class names also appear.
    let body_start = rendered.find("<div class=\"reschedule-banner\"").unwrap();
    let slots_outer_start = rendered.find("<div class=\"slots-outer\"").unwrap();
    assert!(
        body_start < slots_outer_start,
        "Reschedule banner div must appear before slots-outer div to avoid flex layout breakage"
    );
}

/// Render slots.html on the first bookable month, where the handler passes
/// `prev_month` as `None`, and verify the calendar payload carries an empty
/// string rather than the literal "none".
///
/// Regression test for the same class as the `default(value='')` bug above:
/// minijinja renders a Rust `None` as "none", and `default()` does not
/// catch it because the value is defined, just none. The JS guard then saw
/// a truthy "none" and drew a back arrow pointing at ?month=none.
#[test]
fn slots_template_renders_absent_prev_month_as_empty() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);
    let tmpl = env
        .get_template("slots.html")
        .expect("slots.html should load");

    // Exactly what build_month_params hands the template.
    let render = |prev: Option<String>| {
        tmpl.render(context! {
            event_type => context! { slug => "intro", title => "Intro Call", duration_min => 30 },
            host_name => "Alice",
            username => "alice",
            days => Vec::<minijinja::Value>::new(),
            available_dates => Vec::<String>::new(),
            month_label => "March 2026",
            month_year => "2026-03",
            prev_month => prev,
            next_month => Some("2026-04".to_string()),
            first_weekday => 0,
            days_in_month => 31,
            today_date => "2026-03-14",
            guest_tz => "UTC",
        })
        .expect("slots.html should render")
    };
    let payload_line = |rendered: &str, key: &str| {
        rendered
            .lines()
            .find(|l| l.contains(&format!("\"{key}\"")))
            .unwrap_or_else(|| panic!("the calendar payload must carry {key}"))
            .trim()
            .to_string()
    };

    let first_month = render(None);
    assert_eq!(
        payload_line(&first_month, "prevMonth"),
        "\"prevMonth\": \"\",",
        "an absent previous month must serialise as an empty string, not \"none\""
    );
    assert!(
        first_month.contains("\"hasPrevMonth\": false"),
        "hasPrevMonth must be false when there is no previous month"
    );

    // Control: a previous month that does exist must still come through,
    // so this cannot pass by blanking prevMonth unconditionally.
    let later_month = render(Some("2026-02".to_string()));
    assert_eq!(
        payload_line(&later_month, "prevMonth"),
        "\"prevMonth\": \"2026-02\","
    );
    assert!(
        later_month.contains("\"hasPrevMonth\": true"),
        "hasPrevMonth must be true when a previous month exists"
    );

    // The forward arrow is the already-correct sibling: same construct,
    // unchanged by this fix, asserted so the two stay in step.
    assert_eq!(
        payload_line(&first_month, "nextMonth"),
        "\"nextMonth\": \"2026-04\","
    );

    // The AJAX path must gate on the flag, matching the forward arrow,
    // so a future regression in the payload cannot resurrect the arrow.
    assert!(
        first_month.contains("if (data.hasPrevMonth && data.prevMonth)"),
        "the AJAX back arrow must gate on hasPrevMonth, as the forward arrow does"
    );
}

#[test]
fn book_template_phone_field_degrades_without_javascript() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);
    let tmpl = env
        .get_template("book.html")
        .expect("book.html should load");

    let render = |mode: &str| {
        tmpl.render(context! {
            event_type => context! { slug => "intro", title => "Intro Call", duration_min => 30 },
            sms_phone_mode => mode,
            phone_default_country => "+33",
            form_phone => "",
            host_name => "Alice",
            username => "alice",
        })
        .expect("book.html should render")
    };

    let on = render("optional");
    // The visible input keeps name="phone", so a browser that never runs
    // the widget still posts the number and the server still normalises it.
    assert!(
        on.contains(r#"id="phone" name="phone""#),
        "the phone input must keep its own name for the no-JS path"
    );
    assert!(
        on.contains("/static/intl-tel-input/intlTelInput.min.js"),
        "the widget script should be referenced"
    );
    assert!(
        on.contains("loadUtils"),
        "utils.js should be lazy-loaded, not linked eagerly"
    );
    assert!(
        !on.contains(r#"<script src="/static/intl-tel-input/utils.js">"#),
        "utils.js must not be fetched eagerly on every booking page"
    );
    // The dial code the operator configured is what seeds the country when
    // the browser declines to name one.
    assert!(
        on.contains(r#"DEFAULT_DIAL_CODE = "+33""#),
        "the configured dial code should reach the seed logic"
    );
    // Shared dial codes (+1, +7, +44) resolve by priority. Without this the
    // name-sorted list makes +1 American Samoa.
    assert!(
        on.contains("c.priority") && on.contains("best.priority"),
        "dial-code lookup must prefer the primary country"
    );
    // getNumber() returns "" until utils.js lands, so sync() has to build
    // the number from the selected country in the meantime. Without this a
    // guest who submits inside that window posts an empty phone.
    assert!(
        on.contains("getSelectedCountryData"),
        "sync() must fall back to the picker's country before utils.js loads"
    );
    assert!(
        on.contains("iti.promise"),
        "a re-sync must replace the stopgap once libphonenumber is in"
    );

    // Off means the field, the widget and its stylesheet are all absent.
    let off = render("off");
    assert!(!off.contains(r#"id="phone""#), "no phone field when off");
    // The theme block for .iti lives in base.html and is always present,
    // exactly like the cap-widget one. What must not happen is fetching
    // the bundle on a page with no phone field.
    assert!(
        !off.contains("/static/intl-tel-input/"),
        "no widget asset should be fetched when phone collection is off"
    );
}

#[tokio::test]
async fn public_profile_returns_200() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/u/testuser")).await.unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Test Meeting"),
        "Public profile should list event types"
    );
}

#[tokio::test]
async fn public_profile_trailing_slash_redirects_to_working_canonical_page() {
    let (app, _, _, _) = setup_test_app().await;
    for query in ["", "?lang=fr&next=https%3A%2F%2Fexample.com%2F&tag=a&tag=b"] {
        let response = app
            .clone()
            .oneshot(get(&format!("/u/testuser/{query}")))
            .await
            .unwrap();
        assert_eq!(response.status(), 308);
        let location = response.headers()["location"].to_str().unwrap();
        assert_eq!(location, format!("/u/testuser{query}"));
        let canonical = app.clone().oneshot(get(location)).await.unwrap();
        assert_eq!(canonical.status(), 200);
        assert!(body_string(canonical).await.contains("Test Meeting"));
    }
}

#[tokio::test]
async fn public_profile_trailing_slash_head_preserves_encoded_path_and_query() {
    let (app, _, _, _) = setup_test_app().await;
    let request = axum::http::Request::builder()
        .method("HEAD")
        .uri("/u/test%75ser/?value=%2F%3F%26")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 308);
    assert_eq!(
        response.headers()["location"],
        "/u/test%75ser?value=%2F%3F%26"
    );
    assert!(body_string(response).await.is_empty());
}

#[tokio::test]
async fn public_slots_page_returns_200() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/u/testuser/test-meeting")).await.unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("Test Meeting"),
        "Slots page should show event type title"
    );
}

/// #265: the clock-change note is computed by the slot page's script, and
/// the suite has no JavaScript runner, so this pins the rules review asked
/// for in the source itself.
#[test]
fn slots_offset_note_script_keeps_its_rules() {
    let src = std::fs::read_to_string("templates/slots.html").unwrap();
    // English change dates read "25 October", not "October 25".
    assert!(src.contains("if (pageLang === 'en') pageLang = 'en-GB';"));
    // On the change day itself, slots before the change keep the old
    // offset, so that day gets the range wording.
    assert!(src.contains("if (c && c.change === ds) return rangeNoteText(c);"));
    // Intl's "GMT" at zero offset reads "UTC+0", like the picker labels.
    assert!(src.contains("return /^UTC([+-]0(:00)?)?$/.test(s) ? 'UTC+0' : s;"));
    // Week and column views both look only at days with times.
    assert!(src.contains("return (slotData[d.dateStr] || []).length > 0;"));
    assert!(src.contains("if (!slots || slots.length === 0) continue;"));
}

/// #265: the guest picker offers every zone, whatever the guest's own one,
/// and selects the guest's exactly once. The page also carries what the
/// clock-change note needs.
#[tokio::test]
async fn slots_page_offers_every_timezone_and_selects_the_guest_one() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .clone()
        .oneshot(get("/u/testuser/test-meeting?tz=Europe%2FBucharest"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert_eq!(selected_options(&body, "tz-select"), ["Europe/Bucharest"]);
    assert_eq!(body.matches(r#"value="Europe&#x2f;Bucharest""#).count(), 1);
    assert!(body.contains(r#"<optgroup label="Common timezones">"#));
    assert!(body.contains(r#"id="slot-offset-note" hidden"#));
    assert!(body.contains(r#""lang": "en""#));
    assert!(body.contains(r#""offsetNoteDay": "Times on %1 are %2 (clocks change on %3).""#));
    assert!(
        body.contains(r#""offsetNoteRange": "Clocks change on %1: from then on, times are %2.""#)
    );

    // A guest elsewhere can still pick Bucharest.
    let response = app
        .oneshot(get("/u/testuser/test-meeting?tz=UTC"))
        .await
        .unwrap();
    let body = body_string(response).await;
    assert_eq!(selected_options(&body, "tz-select"), ["UTC"]);
    assert!(body.contains(r#"value="Europe&#x2f;Bucharest""#));
}

#[tokio::test]
async fn nonexistent_profile_returns_404_or_empty() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/u/nonexistent")).await.unwrap();
    // Should return 404 or a page indicating no user found
    let status = response.status();
    assert!(
        status == 404 || status == 200,
        "Nonexistent user should return 404 or 200 with empty page"
    );
}

// --- Legacy routes ---

#[tokio::test]
async fn legacy_slot_route_returns_200() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/test-meeting")).await.unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Test Meeting"));
}

// --- Private event types ---

#[tokio::test]
async fn private_event_type_hidden_from_profile() {
    let (app, pool, _, _) = setup_test_app().await;

    let et_id = uuid::Uuid::new_v4().to_string();
    let account_id: String = sqlx::query_scalar("SELECT id FROM accounts LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, enabled, visibility) VALUES (?, ?, 'secret', 'Secret Meeting', 30, 1, 'private')")
        .bind(&et_id)
        .bind(&account_id)
        .execute(&pool)
        .await
        .unwrap();

    let response = app.oneshot(get("/u/testuser")).await.unwrap();
    let body = body_string(response).await;
    assert!(
        !body.contains("Secret Meeting"),
        "Private event type should not appear on public profile"
    );
}

// --- Book form page ---

#[tokio::test]
async fn book_form_page_returns_200() {
    let (app, _, _, _) = setup_test_app().await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let date_str = next_monday.format("%Y-%m-%d").to_string();

    let response = app
        .oneshot(get(&format!(
            "/u/testuser/test-meeting/book?date={}&time=10:00",
            date_str
        )))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Test Meeting"));
    assert!(body.contains("Confirm booking") || body.contains("confirm"));
    assert!(
        body.contains("id=\"name\" name=\"name\" required maxlength=\"255\" value=\"\""),
        "anonymous name should remain empty"
    );
    assert!(
        body.contains("id=\"email\" name=\"email\" required maxlength=\"255\" value=\"\""),
        "anonymous email should remain empty"
    );
}

#[tokio::test]
async fn authenticated_booking_form_prefills_guest_identity() {
    let (app, pool, _, _) = setup_test_app().await;
    let (_, session) =
        insert_test_user_with_session(&pool, "Alice Example", "alice@example.com", "alice").await;

    let response = app
        .oneshot(get_authed(&booking_form_uri(None), &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("value=\"Alice Example\""));
    assert!(body.contains("value=\"alice@example.com\""));
}

// --- Legacy book form ---

#[tokio::test]
async fn legacy_book_form_returns_200() {
    let (app, _, _, _) = setup_test_app().await;

    let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
    let mut next_monday = now.date() + Duration::days(1);
    while next_monday.weekday() != chrono::Weekday::Mon {
        next_monday += Duration::days(1);
    }
    let date_str = next_monday.format("%Y-%m-%d").to_string();

    let response = app
        .oneshot(get(&format!(
            "/test-meeting/book?date={}&time=10:00",
            date_str
        )))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

// --- Nonexistent routes ---

#[tokio::test]
async fn nonexistent_event_type_slug_returns_not_found() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .oneshot(get("/u/testuser/nonexistent-slug"))
        .await
        .unwrap();
    let status = response.status();
    let body = body_string(response).await;
    assert!(
        status == 404 || body.contains("not found") || body.contains("Not found"),
        "Nonexistent slug should return error"
    );
}

// --- Group profile (no groups in test data, should handle gracefully) ---

#[tokio::test]
async fn team_profile_nonexistent_returns_404_or_empty() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/team/nonexistent")).await.unwrap();
    let status = response.status();
    assert!(
        status == 404 || status == 200,
        "Nonexistent group should return 404 or empty page"
    );
}

// --- Slots with month parameter ---

#[tokio::test]
async fn slots_with_month_param() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .oneshot(get("/u/testuser/test-meeting?month=2026-06"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("June") || body.contains("2026"));
}

// --- Slots with timezone parameter ---

#[tokio::test]
async fn slots_with_tz_param() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app
        .oneshot(get("/u/testuser/test-meeting?tz=Europe/Paris"))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Paris") || body.contains("Europe"));
}

// --- Booking form only asks for a phone number when SMS is enabled ---

#[tokio::test]
async fn book_form_shows_the_phone_field_only_when_sms_is_enabled() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let date = (chrono::Utc::now() + chrono::Duration::days(3))
        .format("%Y-%m-%d")
        .to_string();
    let uri = format!(
        "/u/testuser/test-meeting/book?date={}&time=10:00&tz=UTC",
        date
    );

    let response = app.clone().oneshot(get(&uri)).await.unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        !body.contains("name=\"phone\""),
        "phone field should be absent by default"
    );

    sqlx::query("UPDATE event_types SET sms_phone_mode = 'optional' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();

    let response = app.oneshot(get(&uri)).await.unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(
        body.contains("name=\"phone\""),
        "phone field should appear once SMS is enabled"
    );
    // The country code shown in the hint comes from the SMS config.
    assert!(body.contains(crate::sms::phone::DEFAULT_COUNTRY_CODE));
}

#[test]
fn book_template_sets_the_custom_wasm_url_before_the_widget() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);
    let tmpl = env
        .get_template("book.html")
        .expect("book.html should load");
    let render = |wasm_url: &str| {
        tmpl.render(context! {
            event_type => context! { slug => "intro", title => "Intro Call", duration_min => 30 },
            sms_phone_mode => "off",
            host_name => "Alice",
            username => "alice",
            captcha_enabled => true,
            captcha_api_endpoint => "https://cap.example.com/key/",
            captcha_widget_url => "https://cap.example.com/assets/widget.js",
            captcha_wasm_url => wasm_url,
        })
        .expect("book.html should render")
    };

    let set = render("https://cap.example.com/assets/cap_wasm_bg.wasm");
    let assignment =
        r#"window.CAP_CUSTOM_WASM_URL = "https://cap.example.com/assets/cap_wasm_bg.wasm";"#;
    let at = set.find(assignment).expect("the WASM URL should be set");
    let widget = set
        .find("widget.js")
        .expect("the widget script should be referenced");
    assert!(
        at < widget,
        "the WASM URL must be set before the widget loads"
    );

    // A value that tries to close the script is JSON-escaped, not emitted raw.
    let hostile = render("https://x.example.com/</script><script>alert(1)</script>");
    assert!(!hostile.contains("</script><script>alert(1)"));

    let unset = render("");
    assert!(!unset.contains("CAP_CUSTOM_WASM_URL"));
}

#[test]
fn month_slot_window_covers_the_trailing_week() {
    // #228: on Monday 2026-09-28 the week view shows Sep 28 to Oct 4.
    // The September payload must carry Friday Oct 2 and Sunday Oct 4,
    // not stop one day past the month end.
    let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
    let today = d(9, 28);
    let (start, ahead) = month_slot_window(d(9, 1), d(9, 30), today);
    assert_eq!(start, 0, "a month already under way starts today");
    let last = today + Duration::days((start + ahead - 1) as i64);
    assert!(
        last >= d(10, 4),
        "window must reach the grid's Sunday, got {last}"
    );

    // A future month whose 1st falls mid-week also covers the leading
    // Monday, so the week view opened on that month is complete.
    let (start, ahead) = month_slot_window(d(10, 1), d(10, 31), today);
    let first = today + Duration::days(start as i64);
    let last = today + Duration::days((start + ahead - 1) as i64);
    assert!(
        first <= d(9, 28),
        "window must start on the grid's Monday, got {first}"
    );
    assert!(
        last >= d(11, 1),
        "window must reach the grid's Sunday, got {last}"
    );
}

#[test]
fn build_month_params_hides_next_month_past_horizon() {
    let today = host_today();
    let (_, _, _, _, next_month, _, _, _, _) = build_month_params(
        today.year(),
        today.month(),
        Tz::UTC,
        Tz::UTC,
        "en",
        horizon_window(Some(0)),
    );
    assert!(
        next_month.is_none(),
        "a horizon of 0 must drop the next-month link"
    );
}

#[test]
fn build_month_params_keeps_next_month_within_horizon() {
    let today = host_today();
    let (_, _, _, _, far, _, _, _, _) = build_month_params(
        today.year(),
        today.month(),
        Tz::UTC,
        Tz::UTC,
        "en",
        horizon_window(Some(400)),
    );
    assert!(far.is_some(), "a horizon past next month keeps the link");
    let (_, _, _, _, unbounded, _, _, _, _) = build_month_params(
        today.year(),
        today.month(),
        Tz::UTC,
        Tz::UTC,
        "en",
        BookingWindow::default(),
    );
    assert!(unbounded.is_some(), "NULL horizon keeps the link");
}

#[test]
fn build_month_params_survives_an_absurdly_large_horizon() {
    let today = host_today();
    let (_, _, _, _, next, _, _, _, _) = build_month_params(
        today.year(),
        today.month(),
        Tz::UTC,
        Tz::UTC,
        "en",
        horizon_window(Some(i32::MAX)),
    );
    assert!(
        next.is_some(),
        "an unrepresentable horizon must not bound the month nav"
    );
}
