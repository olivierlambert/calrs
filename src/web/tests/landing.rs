//! The public landing pages (`/u/{username}`, `/team/{slug}`): the team
//! page's per-type roster, the duration dials, the sections, the host's
//! facts and usual hours.

use super::*;

#[test]
fn first_names_skip_abbreviated_titles() {
    assert_eq!(first_name("Jane Doe"), "Jane");
    assert_eq!(first_name("Dr. Jane Doe"), "Jane");
    assert_eq!(first_name("Prof. Dr. Hans Meier"), "Hans");
    assert_eq!(first_name("  Ana  "), "Ana");
    assert_eq!(first_name("St."), "St.");
}

#[tokio::test]
async fn team_page_shows_each_type_roster_and_first_names() {
    let (app, pool, _, _) = setup_test_app().await;
    let (team_id, et_id) = seed_team_event_type(&pool).await;
    let (other_id, _) =
        insert_test_user_with_session(&pool, "Zoe Quartz", "zoe@example.com", "zoe").await;
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')")
        .bind(&team_id)
        .bind(&other_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) VALUES (?, ?, 0)",
    )
    .bind(&et_id)
    .bind(&other_id)
    .execute(&pool)
    .await
    .unwrap();

    let page = body_string(
        app.clone()
            .oneshot(get("/team/horizon-team"))
            .await
            .unwrap(),
    )
    .await;
    assert!(
        page.contains(r#"<span class="lp-member-name">Zoe</span>"#),
        "first name under the face"
    );
    let with = page
        .split(r#"<div class="lp-with">"#)
        .nth(1)
        .expect("a roster line");
    let with = with.split("</div>").next().unwrap();
    assert!(with.contains("with one of"), "{with}");
    assert!(with.contains(r#"data-face="Test User""#), "{with}");
    // A screen reader hears the names, not the initials.
    assert!(
        with.contains(r#"<span class="lp-sr">Test User</span>"#),
        "{with}"
    );
    assert!(with.contains(r#"<span class="lp-with-faces" aria-hidden="true">"#));
    assert!(
        !with.contains("Zoe"),
        "an excluded member is not in the roster: {with}"
    );

    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let page = body_string(app.oneshot(get("/team/horizon-team")).await.unwrap()).await;
    assert!(page.contains("with everyone"));
}

/// A meeting's length is drawn as clock dials, one per hour (at most three),
/// the last one filled in proportion, with the exact length written under
/// them and spoken in words.
#[test]
fn landing_dials_draw_one_dial_per_hour() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);
    // The landing header includes the public logo partial; no logo here.
    register_company_logo(
        &mut env,
        std::env::temp_dir().join(format!("calrs_no_logo_{}", uuid::Uuid::new_v4())),
    );
    let render = |minutes: i32| {
        let html = env
            .get_template("profile.html")
            .unwrap()
            .render(context! {
                host_name => "Alice",
                username => "alice",
                cards => vec![context! {
                    title => "Meetings",
                    count => 1,
                    items => vec![context! { slug => "m", title => "M", duration_min => minutes }],
                }],
                topic_count => 1,
                lang => "en",
            })
            .unwrap();
        let dur = html
            .split(r#"<span class="lp-dur""#)
            .nth(1)
            .unwrap()
            .to_string();
        let dur = dur
            .split(r#"<div class="lp-main">"#)
            .next()
            .unwrap()
            .to_string();
        let dashes: Vec<String> = dur
            .split(r#"stroke-dasharray=""#)
            .skip(1)
            .map(|s| s.split(' ').next().unwrap().to_string())
            .collect();
        (dur, dashes)
    };

    let (dur, dashes) = render(15);
    assert!(dur.contains("lp-dials-1") && dur.contains(r#"aria-label="15 minutes""#));
    assert_eq!(dashes, ["25.0"]);
    assert!(dur.contains(">15 min</span>"));

    let (dur, dashes) = render(60);
    assert!(dur.contains(r#"aria-label="1 hour""#) && dur.contains(">1 h</span>"));
    assert_eq!(dashes, ["100"]);

    let (dur, dashes) = render(90);
    assert!(dur.contains("lp-dials-2") && dur.contains(r#"aria-label="1 hour 30 minutes""#));
    assert_eq!(dashes, ["100", "50.0"]);
    assert!(dur.contains(">1 h 30 min</span>"));

    let (_, dashes) = render(135);
    assert_eq!(dashes, ["100", "100", "25.0"]);

    // From three hours on, three full dials and the label says the rest.
    let (dur, dashes) = render(240);
    assert!(dur.contains("lp-dials-3") && dur.contains(">4 h</span>"));
    assert_eq!(dashes, ["100", "100", "100"]);
    let (dur, dashes) = render(195);
    assert!(dur.contains(r#"aria-label="3 hours 15 minutes""#));
    assert_eq!(dashes.len(), 3);
}

/// Without sections the profile lists its meetings in one "Meetings" card
/// with their count, each with how the guest meets and whether requests
/// are confirmed by hand, under the host's usual hours. No next-free hint
/// is asked for any more.
#[tokio::test]
async fn profile_lists_one_card_with_the_decision_facts_and_the_hours() {
    let (app, pool, _, et_id) = setup_test_app().await;
    sqlx::query(
        "UPDATE event_types SET location_type = 'phone', requires_confirmation = 1 WHERE id = ?",
    )
    .bind(&et_id)
    .execute(&pool)
    .await
    .unwrap();
    let page = body_string(app.oneshot(get("/u/testuser")).await.unwrap()).await;
    assert!(
        page.contains(r#"<h2>Meetings<span class="num">1</span></h2>"#),
        "{page}"
    );
    assert!(page.contains("<span>Phone call</span>"));
    assert!(page.contains("On request"));
    assert!(page.contains("Usually available Monday to Friday, 09:00 to 17:00 (UTC time)"));
    assert!(!page.contains("next-free"));
    // One meeting: no line on how booking works.
    assert!(!page.contains(r#"<p class="lp-how">"#));
}

/// Sections become cards in the order of the page, the meetings without a
/// section last under "Other meetings"; the arrange page's moves change
/// that order.
#[tokio::test]
async fn sections_are_cards_in_the_order_the_host_sets() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let second = insert_personal_et(&pool, &user_id, "second").await;
    let third = insert_personal_et(&pool, &user_id, "third").await;
    for (position, (id, section, title)) in [
        (&et_id, Some("Working together"), "First"),
        (&second, None, "Second"),
        (&third, Some("Getting started"), "Third"),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query(
            "UPDATE event_types SET section = ?, title = ?, page_position = ? WHERE id = ?",
        )
        .bind(section)
        .bind(title)
        .bind(position as i64)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    let order = |page: &str, names: &[&str]| -> Vec<usize> {
        names
            .iter()
            .map(|n| page.find(n).unwrap_or(usize::MAX))
            .collect()
    };
    let page = body_string(app.clone().oneshot(get("/u/testuser")).await.unwrap()).await;
    let at = order(
        &page,
        &["Working together", "Getting started", "Other meetings"],
    );
    assert!(at[0] < at[1] && at[1] < at[2], "{at:?}");
    assert!(
        page.contains(r#"<p class="lp-how">"#),
        "several meetings explain booking"
    );

    // Move "Getting started" above "Working together".
    let csrf = "c";
    let res = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/order",
            &session,
            csrf,
            &format!("_csrf={csrf}&kind=section&target=Getting+started&dir=up"),
        ))
        .await
        .unwrap();
    assert!(res.status().is_redirection());
    let page = body_string(app.clone().oneshot(get("/u/testuser")).await.unwrap()).await;
    let at = order(
        &page,
        &["Getting started", "Working together", "Other meetings"],
    );
    assert!(at[0] < at[1] && at[1] < at[2], "{at:?}");

    // The arrange page lists them; a move without the CSRF token is refused.
    let arrange = body_string(
        app.clone()
            .oneshot(get_authed("/dashboard/event-types/order", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(arrange.contains("Getting started") && arrange.contains("Second"));
    let refused = app
        .oneshot(post_form(
            "/dashboard/event-types/order",
            &session,
            csrf,
            "kind=section&target=Working+together&dir=up",
        ))
        .await
        .unwrap();
    assert_eq!(refused.status(), 403);
}

/// A move names an event type by id, but only the page's own types are
/// rewritten: another user's type keeps its position.
#[tokio::test]
async fn moves_never_reach_another_page() {
    let (app, pool, _, _) = setup_test_app().await;
    let (other_id, other_session) =
        insert_test_user_with_session(&pool, "Other", "other@example.com", "other").await;
    insert_personal_et(&pool, &other_id, "mine").await;
    let csrf = "c";
    let res = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/order",
            &other_session,
            csrf,
            &format!("_csrf={csrf}&kind=type&target=x&dir=up"),
        ))
        .await
        .unwrap();
    assert!(res.status().is_redirection());
    let placed: Vec<Option<i64>> = sqlx::query_scalar(
        "SELECT et.page_position FROM event_types et JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id WHERE u.username = 'testuser'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(placed.iter().all(Option::is_none), "{placed:?}");
}

/// A team's page order belongs to its admins: a plain member is refused.
#[tokio::test]
async fn only_team_admins_arrange_the_team_page() {
    let (app, pool, admin_session, _) = setup_test_app().await;
    let (member_id, member_session) =
        insert_test_user_with_session(&pool, "Member", "member@example.com", "member").await;
    let (team_id, _) =
        insert_team_with_et(&pool, "crew", "crew-call", &[(&member_id, "member")]).await;
    let uri = format!("/dashboard/teams/{team_id}/order");
    let res = app
        .clone()
        .oneshot(get_authed(&uri, &member_session))
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    let csrf = "c";
    let res = app
        .clone()
        .oneshot(post_form(
            &uri,
            &member_session,
            csrf,
            &format!("_csrf={csrf}&kind=section&target=x&dir=up"),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
    // The global admin may.
    let res = app.oneshot(get_authed(&uri, &admin_session)).await.unwrap();
    assert_eq!(res.status(), 200);
}

/// "Based in" and the spoken languages are saved from Profile & Settings
/// and shown under the name, the languages named in the visitor's language.
#[tokio::test]
async fn based_in_and_languages_show_under_the_name() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "c";
    let res = app
        .clone()
        .oneshot(post_form(
            "/dashboard/settings",
            &session,
            csrf,
            &format!(
                "_csrf={csrf}&name=Test+User&based_in=++Lyon%0A&spoken_languages_shown=1\
                 &spoken_languages=fr&spoken_languages=en&spoken_languages=zz&avail_schedule="
            ),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let (based, spoken): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT based_in, spoken_languages FROM users WHERE username = 'testuser'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(based.as_deref(), Some("Lyon"));
    assert_eq!(spoken.as_deref(), Some("en,fr"));
    let page = body_string(
        app.clone()
            .oneshot(get_with_language("/u/testuser", "fr"))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains("Localisation : Lyon"), "{page}");
    assert!(page.contains("Parle anglais et français"));
    // An older settings form without the languages keeps them.
    app.clone()
        .oneshot(post_form(
            "/dashboard/settings",
            &session,
            csrf,
            &format!("_csrf={csrf}&name=Test+User&avail_schedule="),
        ))
        .await
        .unwrap();
    let spoken: Option<String> =
        sqlx::query_scalar("SELECT spoken_languages FROM users WHERE username = 'testuser'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(spoken.as_deref(), Some("en,fr"));
}

/// The event type form saves the section, trimmed; an empty one clears it.
#[tokio::test]
async fn the_event_type_form_saves_the_section() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let form = app
        .clone()
        .oneshot(get_authed(
            "/dashboard/event-types/test-meeting/edit",
            &session,
        ))
        .await
        .unwrap();
    let form = body_string(form).await;
    assert!(form.contains(r#"name="section""#));
    assert!(form.contains("Change the order"));
    let save = |section: &str| {
        format!(
            "_csrf=c&title=Test+Meeting&slug=test-meeting&duration_min=30&buffer_before=0\
             &buffer_after=0&min_notice_min=0&location_type=link\
             &location_value=https%3A%2F%2Fmeet.example.com&visibility=public\
             &avail_days=1,2,3,4,5&avail_start=09:00&avail_end=17:00&section={section}"
        )
    };
    let res = app
        .clone()
        .oneshot(post_form(
            "/dashboard/event-types/test-meeting/edit",
            &session,
            "c",
            &save("++Getting++started+"),
        ))
        .await
        .unwrap();
    assert!(res.status().is_redirection(), "{}", res.status());
    let section: Option<String> =
        sqlx::query_scalar("SELECT section FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(section.as_deref(), Some("Getting started"));
    app.oneshot(post_form(
        "/dashboard/event-types/test-meeting/edit",
        &session,
        "c",
        &save(""),
    ))
    .await
    .unwrap();
    let section: Option<String> =
        sqlx::query_scalar("SELECT section FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(section, None);
}

/// The section field suggests the user's own sections and those of the
/// teams they admin, never another user's.
#[tokio::test]
async fn section_suggestions_stay_with_their_owner() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let (other_id, _) =
        insert_test_user_with_session(&pool, "Other", "other@example.com", "other").await;
    let theirs = insert_personal_et(&pool, &other_id, "theirs").await;
    for (id, section) in [(&et_id, "Mine to keep"), (&theirs, "Secret plans")] {
        sqlx::query("UPDATE event_types SET section = ? WHERE id = ?")
            .bind(section)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let form = body_string(
        app.oneshot(get_authed("/dashboard/event-types/new", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(form.contains(r#"<option value="Mine to keep">"#), "{form}");
    assert!(!form.contains("Secret plans"));
}
