//! Creating an event type saves every setting its edit form saves: the
//! scheduling mode, member weights and exclusions, watchers, and the access
//! settings of a team event type, whichever create form it came from.

use super::*;

/// The two forms that create a team event type: the event types form with
/// a team picked, and the team form.
const TEAM_CREATE_URIS: [&str; 2] = [
    "/dashboard/event-types/new",
    "/dashboard/group-event-types/new",
];

async fn test_user_id(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A plain member of `team_id`; returns their user id.
async fn add_member(pool: &SqlitePool, team_id: &str, email: &str) -> String {
    let user_id = insert_role_user(pool, email, "user").await;
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')",
    )
    .bind(team_id)
    .bind(&user_id)
    .execute(pool)
    .await
    .unwrap();
    user_id
}

/// A second team the test user belongs to, offered as a watcher.
async fn watcher_team(pool: &SqlitePool) -> String {
    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO teams (id, name, slug, visibility) VALUES (?, 'Sales', 'sales', 'public')",
    )
    .bind(&team_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')",
    )
    .bind(&team_id)
    .bind(test_user_id(pool).await)
    .execute(pool)
    .await
    .unwrap();
    team_id
}

/// A complete event type form body, `fields` overriding or adding to it.
fn form_body(csrf: &str, fields: &[(&str, &str)]) -> String {
    let mut all: Vec<(&str, &str)> = vec![
        ("_csrf", csrf),
        ("title", "Created Meeting"),
        ("slug", "created-meeting"),
        ("duration_min", "30"),
        ("location_type", "link"),
        ("location_value", "https://meet.example.com"),
        ("avail_days", "1,2,3,4,5"),
        ("avail_start", "09:00"),
        ("avail_end", "17:00"),
    ];
    for (key, value) in fields {
        match all.iter_mut().find(|(k, _)| k == key) {
            Some(field) => field.1 = value,
            None => all.push((key, value)),
        }
    }
    all.iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Posts `fields` to `uri` and expects the save to go through.
async fn save(app: &Router, uri: &str, session: &str, fields: &[(&str, &str)]) {
    let csrf = "csrf-et-create";
    let response = app
        .clone()
        .oneshot(post_form(uri, session, csrf, &form_body(csrf, fields)))
        .await
        .unwrap();
    assert!(
        response.status().is_redirection(),
        "{uri} answered {}: {}",
        response.status(),
        body_string(response).await
    );
}

/// The edit page of a team event type.
fn edit_uri(team_id: &str, slug: &str) -> String {
    format!("/dashboard/group-event-types/{team_id}/{slug}/edit")
}

async fn et_id(pool: &SqlitePool, slug: &str) -> String {
    sqlx::query_scalar("SELECT id FROM event_types WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn scheduling_mode(pool: &SqlitePool, slug: &str) -> String {
    sqlx::query_scalar("SELECT scheduling_mode FROM event_types WHERE slug = ?")
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn weights(pool: &SqlitePool, et_id: &str) -> Vec<(String, i64)> {
    sqlx::query_as(
        "SELECT user_id, weight FROM event_type_member_weights WHERE event_type_id = ? ORDER BY user_id",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn watchers(pool: &SqlitePool, et_id: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT team_id FROM event_type_watchers WHERE event_type_id = ?")
        .bind(et_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// A collective team event type stays collective, from either create form,
/// and its edit page shows it so.
#[tokio::test]
async fn team_create_keeps_the_collective_mode() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;

    for (n, uri) in TEAM_CREATE_URIS.iter().enumerate() {
        let slug = format!("all-hands-{n}");
        save(
            &app,
            uri,
            &session,
            &[
                ("team_id", &team_id),
                ("slug", &slug),
                ("scheduling_mode", "collective"),
            ],
        )
        .await;
        assert_eq!(scheduling_mode(&pool, &slug).await, "collective", "{uri}");
        let page = body_string(
            app.clone()
                .oneshot(get_authed(&edit_uri(&team_id, &slug), &session))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            selected_options(&page, "scheduling_mode"),
            ["collective"],
            "{uri}"
        );
    }
}

/// Round robin stays the default, and a value that is neither mode is
/// stored as round robin on create and on edit.
#[tokio::test]
async fn scheduling_mode_defaults_to_round_robin() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;

    for (n, uri) in TEAM_CREATE_URIS.iter().enumerate() {
        let slug = format!("rr-{n}");
        save(
            &app,
            uri,
            &session,
            &[("team_id", &team_id), ("slug", &slug)],
        )
        .await;
        assert_eq!(scheduling_mode(&pool, &slug).await, "round_robin", "{uri}");

        let slug = format!("odd-{n}");
        let fields = [
            ("team_id", team_id.as_str()),
            ("slug", slug.as_str()),
            ("scheduling_mode", "everyone"),
        ];
        save(&app, uri, &session, &fields).await;
        assert_eq!(scheduling_mode(&pool, &slug).await, "round_robin", "{uri}");
        save(&app, &edit_uri(&team_id, &slug), &session, &fields[1..]).await;
        assert_eq!(scheduling_mode(&pool, &slug).await, "round_robin", "edit");
    }
}

/// A personal event type has no scheduling mode of its own, whatever the
/// form posted.
#[tokio::test]
async fn personal_create_ignores_a_posted_scheduling_mode() {
    let (app, pool, session, _) = setup_test_app().await;
    save(
        &app,
        "/dashboard/event-types/new",
        &session,
        &[("team_id", ""), ("scheduling_mode", "collective")],
    )
    .await;
    assert_eq!(
        scheduling_mode(&pool, "created-meeting").await,
        "round_robin"
    );
}

/// The members a collective event type leaves out (weight 0) and the
/// round-robin priorities are kept from either create form; a user who is
/// not in the team is not given a weight.
#[tokio::test]
async fn team_create_keeps_member_weights_and_exclusions() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    let me = test_user_id(&pool).await;
    let other = add_member(&pool, &team_id, "other@example.com").await;
    let outsider = insert_role_user(&pool, "outsider@example.com", "user").await;

    for (n, uri) in TEAM_CREATE_URIS.iter().enumerate() {
        for (mode, posted) in [
            ("collective", format!("{me}:1,{other}:0,{outsider}:0")),
            ("round_robin", format!("{me}:3,{other}:2,{outsider}:1")),
        ] {
            let slug = format!("{mode}-{n}").replace('_', "-");
            save(
                &app,
                uri,
                &session,
                &[
                    ("team_id", &team_id),
                    ("slug", &slug),
                    ("scheduling_mode", mode),
                    ("member_priorities", &posted),
                ],
            )
            .await;
            let mut expected = match mode {
                "collective" => vec![(me.clone(), 1), (other.clone(), 0)],
                _ => vec![(me.clone(), 3), (other.clone(), 2)],
            };
            expected.sort();
            assert_eq!(
                weights(&pool, &et_id(&pool, &slug).await).await,
                expected,
                "{uri} {mode}"
            );
        }
    }
}

/// Watcher teams picked on the event types form are kept for a team event
/// type, and never attached to a personal one.
#[tokio::test]
async fn event_types_form_keeps_watchers_of_a_team_event_type() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    let sales = watcher_team(&pool).await;

    save(
        &app,
        "/dashboard/event-types/new",
        &session,
        &[
            ("team_id", &team_id),
            ("slug", "watched"),
            ("watcher_team_ids", &sales),
        ],
    )
    .await;
    assert_eq!(
        watchers(&pool, &et_id(&pool, "watched").await).await,
        std::slice::from_ref(&sales)
    );

    save(
        &app,
        "/dashboard/event-types/new",
        &session,
        &[
            ("team_id", ""),
            ("slug", "personal-watched"),
            ("watcher_team_ids", &sales),
        ],
    )
    .await;
    assert!(watchers(&pool, &et_id(&pool, "personal-watched").await)
        .await
        .is_empty());
}

/// The team form keeps the visibility, the reminder and the additional
/// guest limit it shows.
#[tokio::test]
async fn team_form_create_keeps_visibility_reminder_and_guest_limit() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;

    save(
        &app,
        "/dashboard/group-event-types/new",
        &session,
        &[
            ("team_id", &team_id),
            ("visibility", "internal"),
            ("reminder_minutes", "240"),
            ("max_additional_guests", "3"),
        ],
    )
    .await;
    let stored: (String, Option<i32>, i32) = sqlx::query_as(
        "SELECT visibility, reminder_minutes, max_additional_guests FROM event_types WHERE slug = 'created-meeting'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, ("internal".to_string(), Some(240), 3));
}

/// Both create forms show the scheduling mode, the members of each team for
/// the priority card, and the watcher teams.
#[tokio::test]
async fn create_forms_offer_mode_members_and_watchers() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    let other = add_member(&pool, &team_id, "other@example.com").await;
    let sales = watcher_team(&pool).await;

    for uri in TEAM_CREATE_URIS {
        let page = body_string(
            app.clone()
                .oneshot(get_authed(uri, &session))
                .await
                .unwrap(),
        )
        .await;
        assert!(page.contains(r#"<select id="scheduling_mode""#), "{uri}");
        assert!(page.contains(&format!("userId: \"{other}\"")), "{uri}");
        assert!(page.contains(r#"id="watcher-section""#), "{uri}");
        assert!(
            page.contains(&format!(r#"class="watcher-team-check" value="{sales}""#)),
            "{uri}"
        );
    }
}

/// Every `event_types` column of `et_id`, as SQLite quotes it.
async fn row_snapshot(pool: &SqlitePool, et_id: &str) -> Vec<(String, String)> {
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('event_types')")
            .fetch_all(pool)
            .await
            .unwrap();
    let mut out = Vec::new();
    for column in columns {
        let value: String = sqlx::query_scalar(&format!(
            "SELECT quote({column}) FROM event_types WHERE id = ?"
        ))
        .bind(et_id)
        .fetch_one(pool)
        .await
        .unwrap();
        out.push((column, value));
    }
    out
}

/// What an event type's child rows hold, ids left out.
async fn children_snapshot(pool: &SqlitePool, et_id: &str) -> Vec<String> {
    let mut out = Vec::new();
    for query in [
        "SELECT user_id || ':' || weight FROM event_type_member_weights WHERE event_type_id = ? ORDER BY 1",
        "SELECT team_id FROM event_type_watchers WHERE event_type_id = ? ORDER BY 1",
        "SELECT max_bookings || '/' || period FROM booking_frequency_limits WHERE event_type_id = ? ORDER BY 1",
        "SELECT resource_id FROM event_type_resources WHERE event_type_id = ? ORDER BY 1",
        "SELECT kind || ':' || label || ':' || required FROM event_type_questions WHERE event_type_id = ? ORDER BY position",
        "SELECT day_of_week || ' ' || start_time || '-' || end_time FROM availability_rules WHERE event_type_id = ? ORDER BY 1",
    ] {
        let rows: Vec<String> = sqlx::query_scalar(query)
            .bind(et_id)
            .fetch_all(pool)
            .await
            .unwrap();
        out.push(format!("{query}: {rows:?}"));
    }
    out
}

/// A team event type created with every setting filled in is stored
/// exactly as saving the same values on its edit page stores it, from
/// either create form: create drops nothing that edit saves.
#[tokio::test]
async fn team_create_stores_what_edit_stores() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    let me = test_user_id(&pool).await;
    let other = add_member(&pool, &team_id, "other@example.com").await;
    let sales = watcher_team(&pool).await;
    let resource = insert_resource(&pool, "Lab").await;
    let weights = format!("{me}:1,{other}:0");
    let questions = r#"[{"kind":"short_text","label":"Company","required":true}]"#;

    for (n, uri) in TEAM_CREATE_URIS.iter().enumerate() {
        let slug = format!("full-{n}");
        let settings: Vec<(&str, &str)> = vec![
            ("slug", &slug),
            ("description", "Quarterly review"),
            ("duration_min", "45"),
            ("slot_interval_min", "15"),
            ("buffer_before", "5"),
            ("buffer_after", "10"),
            ("min_notice_min", "120"),
            ("scheduling_mode", "collective"),
            ("visibility", "internal"),
            ("reminder_minutes", "240"),
            ("max_additional_guests", "3"),
            ("requires_confirmation", "on"),
            ("pending_expiry_hours", "24"),
            ("booking_horizon_days", "30"),
            ("bookable_from", "2027-01-04"),
            ("bookable_until", "2027-12-20"),
            ("notes_mode", "required"),
            ("questions", questions),
            ("sms_phone_mode", "optional"),
            ("first_slot_only", "on"),
            ("default_calendar_view", "week"),
            ("week_start", "mon"),
            ("cancel_notice_value", "2"),
            ("cancel_notice_unit", "hours"),
            ("reschedule_notice_value", "1"),
            ("reschedule_notice_unit", "days"),
            ("timezone", "Europe/Paris"),
            ("frequency_limits", "2:day"),
            ("resource_ids", &resource),
            ("resource_scheduling_mode", "round_robin"),
            ("watcher_team_ids", &sales),
            ("avail_days", "1,3"),
            ("avail_start", "10:00"),
            ("avail_end", "12:00"),
        ];
        let mut create = settings.clone();
        create.push(("team_id", &team_id));
        create.push(("member_priorities", &weights));
        save(&app, uri, &session, &create).await;
        let id = et_id(&pool, &slug).await;
        let created = (
            row_snapshot(&pool, &id).await,
            children_snapshot(&pool, &id).await,
        );

        // The values are really there, not left at their defaults on both.
        let column = |name: &str| {
            created
                .0
                .iter()
                .find(|(c, _)| c == name)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        assert_eq!(column("scheduling_mode"), "'collective'", "{uri}");
        assert_eq!(column("visibility"), "'internal'", "{uri}");
        assert_eq!(column("reminder_minutes"), "240", "{uri}");
        assert_eq!(column("max_additional_guests"), "3", "{uri}");
        assert_eq!(column("pending_expiry_hours"), "24", "{uri}");
        assert_eq!(column("booking_horizon_days"), "30", "{uri}");
        assert_eq!(column("sms_phone_mode"), "'optional'", "{uri}");
        assert!(created.1[0].contains(&format!("{other}:0")), "{uri}");
        assert!(created.1[1].contains(&sales), "{uri}");
        assert!(created.1[3].contains(&resource), "{uri}");
        assert!(created.1[4].contains("Company"), "{uri}");

        save(&app, &edit_uri(&team_id, &slug), &session, &settings).await;
        let edited = (
            row_snapshot(&pool, &id).await,
            children_snapshot(&pool, &id).await,
        );
        assert_eq!(created, edited, "{uri}");
    }
}

/// Duplicating a collective team event type keeps it collective.
#[tokio::test]
async fn duplicate_keeps_the_collective_mode() {
    let (app, pool, session, _) = setup_test_app().await;
    let (team_id, _) = seed_team_event_type(&pool).await;
    sqlx::query("UPDATE event_types SET scheduling_mode = 'collective' WHERE slug = 'team-sync'")
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "csrf-dup-mode";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/group-event-types/{team_id}/team-sync/duplicate"),
            &session,
            csrf,
            &format!("_csrf={csrf}"),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    assert_eq!(scheduling_mode(&pool, "team-sync-copy").await, "collective");
}

/// A team admin who is not a global admin can only name as watchers the
/// teams they belong to, on create; a watcher an admin attached stays when
/// they edit the event type.
#[tokio::test]
async fn watchers_are_limited_to_the_editors_teams() {
    let (app, pool, _, _) = setup_test_app().await;
    let (team_id, team_et) = seed_team_event_type(&pool).await;
    let (lead, session) =
        insert_test_user_with_session(&pool, "Lead", "lead@example.com", "lead").await;
    sqlx::query(
        "INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Lead', 'lead@example.com', 'UTC', ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&lead)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')",
    )
    .bind(&team_id)
    .bind(&lead)
    .execute(&pool)
    .await
    .unwrap();
    // The test user's team, which the lead is not in.
    let sales = watcher_team(&pool).await;

    for (n, uri) in TEAM_CREATE_URIS.iter().enumerate() {
        let slug = format!("lead-{n}");
        let posted = format!("{sales},{team_id}");
        save(
            &app,
            uri,
            &session,
            &[
                ("team_id", &team_id),
                ("slug", &slug),
                ("watcher_team_ids", &posted),
            ],
        )
        .await;
        assert_eq!(
            watchers(&pool, &et_id(&pool, &slug).await).await,
            std::slice::from_ref(&team_id),
            "{uri}"
        );
    }

    sqlx::query("INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)")
        .bind(&team_et)
        .bind(&sales)
        .execute(&pool)
        .await
        .unwrap();
    let posted = format!("{sales},{team_id}");
    save(
        &app,
        &edit_uri(&team_id, "team-sync"),
        &session,
        &[("slug", "team-sync"), ("watcher_team_ids", &posted)],
    )
    .await;
    let mut kept = watchers(&pool, &team_et).await;
    kept.sort();
    let mut expected = vec![sales, team_id];
    expected.sort();
    assert_eq!(kept, expected);
}
