//! Server-side paging of the growing dashboard lists: the bookings page and
//! its five sections, invites, teams, team event types, team members, the
//! admin Users and Groups lists, and the capped faces of public team pages.

use super::*;

/// HTML attribute values as the templates escape them, read back.
fn unescape(body: &str) -> String {
    body.replace("&amp;", "&").replace("&#x2f;", "/")
}

/// The `<section>` whose heading has `id`.
fn section<'a>(body: &'a str, id: &str) -> &'a str {
    let start = body
        .find(&format!("id=\"{id}\""))
        .unwrap_or_else(|| panic!("no section {id}"));
    let end = start + body[start..].find("</section>").unwrap();
    &body[start..end]
}

fn rows(html: &str) -> usize {
    html.matches("class=\"tt-row\"").count()
}

/// The booking ids an Upcoming or Pending section lists.
fn booking_ids(html: &str) -> Vec<String> {
    html.split("id=\"b-")
        .skip(1)
        .map(|rest| rest[..rest.find('"').unwrap()].to_string())
        .collect()
}

async fn page(app: &Router, session: &str, uri: &str) -> String {
    let response = app.clone().oneshot(get_authed(uri, session)).await.unwrap();
    assert_eq!(response.status(), 200, "{uri}");
    body_string(response).await
}

async fn user_id(pool: &SqlitePool, email: &str) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE email = ?")
        .bind(email)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn insert_at(
    pool: &SqlitePool,
    et_id: &str,
    start: chrono::DateTime<Utc>,
    status: &str,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let end = start + Duration::minutes(30);
    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone,
             start_at, end_at, time_version, status, cancel_token, reschedule_token)
         VALUES (?, ?, ?, 'Guest', 'guest@example.com', 'UTC', ?, ?, 1, ?, ?, ?)",
    )
    .bind(&id)
    .bind(et_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(start.format("%Y-%m-%dT%H:%M:%SZ").to_string())
    .bind(end.format("%Y-%m-%dT%H:%M:%SZ").to_string())
    .bind(status)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn bookings_page_sections_page_independently() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let me = user_id(&pool, "test@example.com").await;
    let other_et = insert_personal_et(&pool, &me, "other").await;
    let now = Utc::now();
    // Upcoming: 60 bookings, tied two by two on their start time.
    let mut upcoming = Vec::new();
    for i in 0..30 {
        let start = now + Duration::days(2) + Duration::hours(i);
        upcoming.push(insert_at(&pool, &et_id, start, "confirmed").await);
        upcoming.push(insert_at(&pool, &other_et, start, "confirmed").await);
    }
    for i in 0..51 {
        let start = now + Duration::days(40) + Duration::hours(i);
        insert_at(&pool, &et_id, start, "pending").await;
    }
    for i in 0..52 {
        let start = now - Duration::days(20) + Duration::hours(i);
        insert_at(&pool, &et_id, start, "confirmed").await;
    }
    for i in 0..53 {
        let start = now + Duration::days(60) + Duration::hours(i);
        insert_at(&pool, &et_id, start, "cancelled").await;
    }

    let first = page(&app, &session, "/dashboard/bookings").await;
    assert!(first.contains("Upcoming: 60. Waiting for your decision: 51."));
    for (id, shown, range) in [
        ("bk-pending", 50, "1 to 50 of 51"),
        ("bk-upcoming", 50, "1 to 50 of 60"),
        ("bk-recent", 50, "1 to 50 of 52"),
        ("bk-cancelled", 50, "1 to 50 of 53"),
    ] {
        let html = section(&first, id);
        assert_eq!(rows(html), shown, "{id}");
        assert!(html.contains(range), "{id}: {range}");
        assert!(html.contains("aria-current=\"page\""), "{id}");
    }
    assert!(section(&first, "bk-upcoming").contains("<span class=\"num\">60</span>"));

    // One section's page leaves the others where they were, and each
    // section's links keep the others' pages.
    let second = page(&app, &session, "/dashboard/bookings?bp=2").await;
    let upcoming_html = unescape(section(&second, "bk-upcoming"));
    assert_eq!(rows(&upcoming_html), 10);
    assert!(upcoming_html.contains("51 to 60 of 60"));
    assert!(upcoming_html.contains("href=\"/dashboard/bookings#bk-upcoming\" rel=\"prev\""));
    let pending_html = unescape(section(&second, "bk-pending"));
    assert_eq!(rows(&pending_html), 50);
    assert!(pending_html.contains("href=\"/dashboard/bookings?pp=2&bp=2#bk-pending\" rel=\"next\""));

    // Every upcoming booking once over the two pages, ties included.
    let mut seen = booking_ids(section(&first, "bk-upcoming"));
    seen.extend(booking_ids(section(&second, "bk-upcoming")));
    let mut expected = upcoming.clone();
    expected.sort();
    let mut sorted = seen.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(seen.len(), 60);
    assert_eq!(sorted, expected);

    // A page past the end is the last one; nonsense is the first.
    let past = page(&app, &session, "/dashboard/bookings?bp=99&xp=abc").await;
    assert!(section(&past, "bk-upcoming").contains("51 to 60 of 60"));
    assert!(section(&past, "bk-cancelled").contains("1 to 50 of 53"));

    let tails = page(&app, &session, "/dashboard/bookings?rp=2&xp=2&pp=2").await;
    assert_eq!(rows(section(&tails, "bk-recent")), 2);
    assert_eq!(rows(section(&tails, "bk-cancelled")), 3);
    assert_eq!(rows(section(&tails, "bk-pending")), 1);
    assert_eq!(rows(section(&tails, "bk-upcoming")), 50);
    let cancelled_html = unescape(section(&tails, "bk-cancelled"));
    assert!(cancelled_html.contains("href=\"/dashboard/bookings?pp=2&rp=2#bk-cancelled\""));
}

/// A user in two teams that both watch an event type holds a claim token
/// through each: the booking is listed and counted once.
#[tokio::test]
async fn claimable_bookings_page_and_are_listed_once() {
    let (app, pool, session, _) = setup_test_app().await;
    let me = user_id(&pool, "test@example.com").await;
    let host = insert_role_user(&pool, "host@example.com", "user").await;
    let (_, team_et) = insert_team_with_et(&pool, "crew", "crew-call", &[(&host, "admin")]).await;
    for slug in ["watch-a", "watch-b"] {
        let team = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO teams (id, name, slug, visibility) VALUES (?, ?, ?, 'public')")
            .bind(&team)
            .bind(slug)
            .bind(slug)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')",
        )
        .bind(&team)
        .bind(&me)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)")
            .bind(&team_et)
            .bind(&team)
            .execute(&pool)
            .await
            .unwrap();
    }
    let now = Utc::now();
    for i in 0..51 {
        let booking = insert_at(
            &pool,
            &team_et,
            now + Duration::days(3) + Duration::hours(i),
            "confirmed",
        )
        .await;
        for _ in 0..2 {
            sqlx::query(
                "INSERT INTO booking_claim_tokens (id, booking_id, user_id, token, expires_at)
                 VALUES (?, ?, ?, ?, datetime('now', '+7 days'))",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&booking)
            .bind(&me)
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    let first = page(&app, &session, "/dashboard/bookings").await;
    let html = section(&first, "bk-claim");
    assert_eq!(rows(html), 50);
    assert!(html.contains("1 to 50 of 51"));
    assert!(html.contains("<span class=\"num\">51</span>"));
    let second = page(&app, &session, "/dashboard/bookings?cp=2").await;
    assert_eq!(rows(section(&second, "bk-claim")), 1);
}

#[tokio::test]
async fn invites_of_an_event_type_are_paged() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let me = user_id(&pool, "test@example.com").await;
    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    // Every invite created in the same second: the id breaks the tie.
    for i in 0..55 {
        sqlx::query(
            "INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, max_uses,
                 created_by_user_id, created_at)
             VALUES (?, ?, ?, ?, 'g@example.com', 1, ?, '2030-01-01 00:00:00')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&et_id)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(format!("Invitee {i:02}"))
        .bind(&me)
        .execute(&pool)
        .await
        .unwrap();
    }
    let names = |body: &str| -> Vec<String> {
        body.split("<strong style=\"font-size: 0.9rem;\">")
            .skip(1)
            .map(|rest| rest[..rest.find('<').unwrap()].to_string())
            .collect()
    };
    let uri = format!("/dashboard/invites/{et_id}");
    let first = page(&app, &session, &uri).await;
    assert_eq!(names(&first).len(), 50);
    assert!(first.contains("1 to 50 of 55"));
    assert!(unescape(&first).contains(&format!(
        "href=\"/dashboard/invites/{et_id}?page=2#invites-sent\""
    )));
    let second = page(&app, &session, &format!("{uri}?page=2")).await;
    let last = page(&app, &session, &format!("{uri}?page=7")).await;
    assert_eq!(names(&second), names(&last));
    let mut all = names(&first);
    all.extend(names(&second));
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 55, "each invite once");
}

#[tokio::test]
async fn invite_links_teams_and_team_event_types_are_paged() {
    let (app, pool, session, _) = setup_test_app().await;
    let me = user_id(&pool, "test@example.com").await;
    let member = insert_role_user(&pool, "member@example.com", "user").await;
    for i in 0..55 {
        let (team, et) = insert_team_with_et(
            &pool,
            &format!("team-{i:02}"),
            &format!("call-{i:02}"),
            &[(&member, "member")],
        )
        .await;
        sqlx::query("UPDATE teams SET name = 'Same name' WHERE id = ?")
            .bind(&team)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE event_types SET title = ? WHERE id = ?")
            .bind(format!("Team call {i:02}"))
            .bind(&et)
            .execute(&pool)
            .await
            .unwrap();
        // An internal event type of the admin's, for the Invite Links page.
        let internal = insert_personal_et(&pool, &me, &format!("internal-{i:02}")).await;
        sqlx::query("UPDATE event_types SET visibility = 'internal', title = ? WHERE id = ?")
            .bind(format!("Internal {i:02}"))
            .bind(&internal)
            .execute(&pool)
            .await
            .unwrap();
    }

    // Invite Links: every internal event type, 50 a page.
    let first = page(&app, &session, "/dashboard/invite-links").await;
    assert_eq!(first.matches("class=\"ui-row et-row\"").count(), 50);
    assert!(first.contains("1 to 50 of 55"));
    let second = page(&app, &session, "/dashboard/invite-links?page=2").await;
    assert_eq!(second.matches("class=\"ui-row et-row\"").count(), 5);

    // Teams, all tied on their name for an admin.
    let first = page(&app, &session, "/dashboard/teams").await;
    assert!(first.contains("1 to 50 of 55"));
    let second = page(&app, &session, "/dashboard/teams?page=9").await;
    assert!(second.contains("51 to 55 of 55"));
    let team_ids = |body: &str| -> Vec<String> {
        body.split("/dashboard/teams/")
            .skip(1)
            .filter_map(|rest| {
                let (id, tail) = rest.split_once('/')?;
                tail.starts_with("settings").then(|| id.to_string())
            })
            .collect()
    };
    let mut all = team_ids(&first);
    all.extend(team_ids(&second));
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 55);

    // Team event types: the admin's own event type heads the first page
    // only, the team ones are paged.
    let first = page(&app, &session, "/dashboard/event-types").await;
    assert!(first.contains("Test Meeting"));
    assert!(first.contains("1 to 50 of 55"));
    let second = page(&app, &session, "/dashboard/event-types?page=2").await;
    assert!(!second.contains("Test Meeting"));
    assert!(second.contains("51 to 55 of 55"));
    assert_eq!(second.matches("class=\"ui-row et-row").count(), 5);

    // A user who belongs to no team sees none of them.
    let (_, outsider_session) =
        insert_test_user_with_session(&pool, "Outsider", "outsider@example.com", "outsider").await;
    let teams = page(&app, &outsider_session, "/dashboard/teams").await;
    assert!(team_ids(&teams).is_empty());
    assert!(!teams.contains("class=\"ui-pager\""));
    let ets = page(&app, &outsider_session, "/dashboard/event-types").await;
    assert!(!ets.contains("Team call 0"));
}

#[tokio::test]
async fn team_settings_page_members_and_post_the_whole_membership() {
    let (app, pool, session, _) = setup_test_app().await;
    let mut members = Vec::new();
    for i in 0..55 {
        members.push(insert_role_user(&pool, &format!("m{i:02}@example.com"), "user").await);
    }
    let roster: Vec<(&str, &str)> = members.iter().map(|id| (id.as_str(), "member")).collect();
    let (team, _) = insert_team_with_et(&pool, "big", "big-call", &roster).await;
    let uri = format!("/dashboard/teams/{team}/settings");
    let first = page(&app, &session, &uri).await;
    assert_eq!(first.matches("class=\"member-row\" data-id=").count(), 50);
    assert!(first.contains("1 to 50 of 55"));
    assert!(unescape(&first).contains(&format!(
        "href=\"/dashboard/teams/{team}/settings?mp=2#members\""
    )));
    // The form posts every member, not only the page drawn.
    let state = &first[first.find("var currentMembers").unwrap()..];
    let state = &state[..state.find(".forEach").unwrap()];
    for id in &members {
        assert!(state.contains(id.as_str()), "{id} carried");
    }
    let second = page(&app, &session, &format!("{uri}?mp=2")).await;
    assert_eq!(second.matches("class=\"member-row\" data-id=").count(), 5);
}

#[tokio::test]
async fn public_team_pages_show_capped_faces() {
    let (app, pool, _, _) = setup_test_app().await;
    let mut roster = Vec::new();
    for i in 0..30 {
        roster.push(insert_role_user(&pool, &format!("face{i:02}@example.com"), "user").await);
    }
    let members: Vec<(&str, &str)> = roster.iter().map(|id| (id.as_str(), "member")).collect();
    insert_team_with_et(&pool, "faces", "faces-call", &members).await;
    let body = body_string(app.clone().oneshot(get("/team/faces")).await.unwrap()).await;
    assert_eq!(body.matches("class=\"lp-member\"").count(), 12);
    assert!(body.contains("+18"));
    assert!(body.contains("30 members"));
    // Each event type names its first faces and how many more there are.
    assert!(body.contains("and 22 more"));
    let slots = body_string(app.oneshot(get("/team/faces/faces-call")).await.unwrap()).await;
    assert!(slots.contains(">+24</span>"));
}

async fn insert_group(pool: &SqlitePool, name: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO groups (id, name) VALUES (?, ?)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn admin_groups_page_search_and_page_their_members() {
    let (app, pool, session, _) = setup_test_app().await;
    for i in 0..30 {
        insert_group(&pool, &format!("Group {i:02}")).await;
    }
    let big = insert_group(&pool, "Zeta big").await;
    for i in 0..25 {
        let user = insert_role_user(&pool, &format!("z{i:02}@example.com"), "user").await;
        sqlx::query("INSERT INTO user_groups (user_id, group_id, weight) VALUES (?, ?, 1)")
            .bind(&user)
            .bind(&big)
            .execute(&pool)
            .await
            .unwrap();
    }
    let groups = |body: &str| {
        section(body, "groups")
            .matches("class=\"group-item\"")
            .count()
    };
    let weights = |body: &str| {
        section(body, "groups")
            .matches("class=\"adm-weight\"")
            .count()
    };

    let first = page(&app, &session, "/dashboard/admin").await;
    assert_eq!(groups(&first), 25);
    assert!(first.contains("1 to 25 of 31"));
    assert!(!section(&first, "groups").contains("Zeta big"));
    // Links keep the Users list where it was.
    let kept = page(&app, &session, "/dashboard/admin?uq=z0&up=1").await;
    assert!(unescape(&kept).contains("href=\"/dashboard/admin?uq=z0&gp=2#groups\""));

    let second = page(&app, &session, "/dashboard/admin?gp=2").await;
    assert_eq!(groups(&second), 6);
    assert!(second.contains("Zeta big"));
    // The big group shows ten members a page, the first page by weight
    // then name.
    assert_eq!(weights(&second), 10);
    assert!(second.contains("z00@example.com"));
    assert!(second.contains("1 to 10 of 25"));
    let members_link = format!("href=\"/dashboard/admin?gp=2&gm={big}&gmp=2#group-{big}\"");
    assert!(unescape(&second).contains(&members_link), "{members_link}");
    let third = page(
        &app,
        &session,
        &format!("/dashboard/admin?gp=2&gm={big}&gmp=3"),
    )
    .await;
    assert_eq!(weights(&third), 5);
    assert!(third.contains("21 to 25 of 25"));
    assert!(third.contains("z24@example.com"));
    // Past the end: the last page of members.
    let past = page(
        &app,
        &session,
        &format!("/dashboard/admin?gp=2&gm={big}&gmp=40"),
    )
    .await;
    assert!(past.contains("21 to 25 of 25"));

    // Search is on the server, literal and case blind.
    let found = page(&app, &session, "/dashboard/admin?gq=zETA").await;
    assert_eq!(groups(&found), 1);
    let none = page(&app, &session, "/dashboard/admin?gq=%25").await;
    assert_eq!(groups(&none), 0);
    assert!(none.contains("No groups match your filter."));
}
