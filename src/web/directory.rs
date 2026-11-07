//! Discover shareable meeting types and keep personal dashboard favorites.

use super::*;

#[derive(sqlx::FromRow, serde::Serialize)]
pub(super) struct DirectoryEntry {
    id: String,
    title: String,
    duration_min: i32,
    owner_name: String,
    owner_slug: String,
    visibility: String,
    url: String,
    favorite: bool,
    #[serde(skip)]
    bookable_until: Option<String>,
    #[serde(skip)]
    timezone: Option<String>,
    /// Hosts of a team meeting whose name or username matched the search,
    /// shown so the match does not look arbitrary. Only filled on the
    /// directory page.
    #[sqlx(skip)]
    matched_hosts: Vec<String>,
}

/// A member who takes bookings for a team meeting: enabled, and not set to
/// weight 0 (the same rule round-robin and collective scheduling use).
struct TeamHost {
    name: String,
    username: String,
}

/// Hosts of the team meetings the viewer may search by person. Members of a
/// private team are only revealed to other members of that team, since the
/// directory lists its internal meetings to every colleague.
///
/// Read as three small lists (the meetings, each team's members in name
/// order, the members set to weight 0) and combined here: one row per
/// meeting and member is tens of thousands of rows on a large instance.
async fn searchable_team_hosts(
    pool: &SqlitePool,
    viewer_id: &str,
) -> Result<HashMap<String, Vec<TeamHost>>, sqlx::Error> {
    let meetings: Vec<(String, String)> = sqlx::query_as(
        "SELECT et.id, et.team_id
         FROM event_types et
         JOIN teams t ON t.id = et.team_id
         WHERE et.enabled = 1
           AND (t.visibility = 'public'
                OR EXISTS (SELECT 1 FROM team_members m
                           WHERE m.team_id = t.id AND m.user_id = ?))",
    )
    .bind(viewer_id)
    .fetch_all(pool)
    .await?;
    if meetings.is_empty() {
        return Ok(HashMap::new());
    }
    let members: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT tm.team_id, u.id, u.name, COALESCE(u.username, '')
         FROM team_members tm
         JOIN users u ON u.id = tm.user_id
         WHERE u.enabled = 1
         ORDER BY u.name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await?;
    let zero_weight: Vec<(String, String)> = sqlx::query_as(
        "SELECT event_type_id, user_id FROM event_type_member_weights WHERE weight <= 0",
    )
    .fetch_all(pool)
    .await?;
    let mut excluded: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    for (et_id, user_id) in zero_weight {
        excluded.entry(et_id).or_default().insert(user_id);
    }
    let mut by_team: HashMap<String, Vec<(String, String, String)>> = HashMap::new();
    for (team_id, user_id, name, username) in members {
        by_team
            .entry(team_id)
            .or_default()
            .push((user_id, name, username));
    }
    let mut hosts: HashMap<String, Vec<TeamHost>> = HashMap::new();
    for (et_id, team_id) in meetings {
        let Some(team) = by_team.get(&team_id) else {
            continue;
        };
        let skip = excluded.get(&et_id);
        let list: Vec<TeamHost> = team
            .iter()
            .filter(|(user_id, _, _)| !skip.is_some_and(|set| set.contains(user_id)))
            .map(|(_, name, username)| TeamHost {
                name: name.clone(),
                username: username.clone(),
            })
            .collect();
        if !list.is_empty() {
            hosts.entry(et_id).or_default().extend(list);
        }
    }
    Ok(hosts)
}

/// Keep the entries matching every search word, literally and case-blind,
/// against the title, the owner (person or team) and the hosts of a team
/// meeting. Records which hosts matched so the page can say why.
fn filter_by_search(
    entries: &mut Vec<DirectoryEntry>,
    search: &str,
    hosts: &HashMap<String, Vec<TeamHost>>,
) {
    let words: Vec<String> = search.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return;
    }
    entries.retain_mut(|entry| {
        let base =
            format!("{} {} {}", entry.title, entry.owner_name, entry.owner_slug).to_lowercase();
        let entry_hosts: Vec<(String, &TeamHost)> = hosts
            .get(&entry.id)
            .map(|list| {
                list.iter()
                    .map(|h| (format!("{} {}", h.name, h.username).to_lowercase(), h))
                    .collect()
            })
            .unwrap_or_default();
        let all_match = words.iter().all(|word| {
            base.contains(word.as_str())
                || entry_hosts
                    .iter()
                    .any(|(text, _)| text.contains(word.as_str()))
        });
        if all_match {
            entry.matched_hosts = entry_hosts
                .iter()
                .filter(|(text, _)| {
                    words
                        .iter()
                        .any(|w| !base.contains(w.as_str()) && text.contains(w.as_str()))
                })
                .map(|(_, h)| h.name.clone())
                .collect();
        }
        all_match
    });
}

/// Use the same visibility rules for discovery, dashboard shortcuts and saves.
/// Internal types are explicitly shareable by any authenticated colleague,
/// including those on private teams. Public types on private teams require a
/// team token, so their bare URLs are not offered here. Private event types
/// remain in their owners' management pages.
pub(super) async fn directory_entries(
    pool: &SqlitePool,
    user_id: &str,
    favorites_only: bool,
    event_type_id: Option<&str>,
) -> Result<Vec<DirectoryEntry>, sqlx::Error> {
    let mut entries: Vec<DirectoryEntry> = sqlx::query_as(
        "SELECT et.id, et.title, et.duration_min, et.visibility,
                CASE WHEN et.team_id IS NULL THEN u.name ELSE t.name END AS owner_name,
                CASE WHEN et.team_id IS NULL THEN u.username ELSE t.slug END AS owner_slug,
                CASE WHEN et.team_id IS NULL THEN '/u/' || u.username || '/' || et.slug
                     ELSE '/team/' || t.slug || '/' || et.slug END AS url,
                f.user_id IS NOT NULL AS favorite,
                et.bookable_until, COALESCE(NULLIF(et.timezone, ''), u.timezone) AS timezone
         FROM event_types et
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN event_type_favorites f ON f.event_type_id = et.id AND f.user_id = ?
         WHERE et.enabled = 1 AND et.visibility IN ('public', 'internal')
           AND ((et.team_id IS NULL AND u.enabled = 1 AND COALESCE(u.username, '') <> '')
                OR (et.team_id IS NOT NULL AND COALESCE(t.slug, '') <> ''
                    AND (t.visibility = 'public' OR et.visibility = 'internal')))
           AND (? = 0 OR f.user_id IS NOT NULL)
           AND (? IS NULL OR et.id = ?)
         ORDER BY owner_name COLLATE NOCASE, et.title COLLATE NOCASE, et.id",
    )
    .bind(user_id)
    .bind(favorites_only)
    .bind(event_type_id)
    .bind(event_type_id)
    .fetch_all(pool)
    .await?;

    entries.retain(|entry| {
        let tz = entry
            .timezone
            .as_deref()
            .and_then(|s| s.parse::<Tz>().ok())
            .unwrap_or_else(server_tz);
        let window = BookingWindow {
            bookable_until: parse_optional_date(entry.bookable_until.as_deref()),
            ..BookingWindow::default()
        };
        !window.is_expired(today_in(tz))
    });
    Ok(entries)
}

#[derive(Default, Deserialize)]
pub(super) struct DirectoryQuery {
    #[serde(default)]
    q: String,
    /// A hand-written `?favorites=1` or `=on` must not be a 400.
    #[serde(default, deserialize_with = "lenient_flag")]
    favorites: bool,
}

fn lenient_flag<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let value = String::deserialize(d)?;
    Ok(matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "on" | "yes"
    ))
}

pub(super) async fn dashboard_directory(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<DirectoryQuery>,
) -> Response {
    let mut entries =
        match directory_entries(&state.pool, &auth_user.user.id, query.favorites, None).await {
            Ok(entries) => entries,
            Err(e) => return internal_error_response("load booking directory", &e),
        };
    // Unicode-aware, literal, multi-word search; no SQL wildcard semantics.
    if !query.q.trim().is_empty() {
        let hosts = match searchable_team_hosts(&state.pool, &auth_user.user.id).await {
            Ok(hosts) => hosts,
            Err(e) => return internal_error_response("load directory team hosts", &e),
        };
        filter_by_search(&mut entries, &query.q, &hosts);
    }
    let tmpl = match state.templates.get_template("dashboard_directory.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("directory template", &e),
    };
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    match tmpl.render(context! {
        sidebar => sidebar_context(&auth_user, "directory"),
        lang => auth_user.lang,
        directory_entries => entries,
        search => query.q.trim(),
        favorites_only => query.favorites,
        impersonating => impersonating,
        impersonating_name => impersonating_name,
    }) {
        Ok(body) => Html(body).into_response(),
        Err(e) => internal_error_response("render booking directory", &e),
    }
}

#[derive(Deserialize)]
pub(super) struct FavoriteForm {
    _csrf: Option<String>,
    favorite: bool,
}

pub(super) async fn set_directory_favorite(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(event_type_id): Path<String>,
    Form(form): Form<FavoriteForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    if form.favorite {
        match directory_entries(&state.pool, &auth_user.user.id, false, Some(&event_type_id)).await
        {
            Ok(entries) if entries.is_empty() => {
                return axum::http::StatusCode::NOT_FOUND.into_response()
            }
            Err(e) => return internal_error_response("check favorite access", &e),
            _ => {}
        }
    }
    // Explicit desired state makes retries and double-clicks idempotent.
    // Removal remains possible after a type becomes unavailable.
    let sql = if form.favorite {
        "INSERT INTO event_type_favorites (user_id, event_type_id) VALUES (?, ?) ON CONFLICT DO NOTHING"
    } else {
        "DELETE FROM event_type_favorites WHERE user_id = ? AND event_type_id = ?"
    };
    if let Err(e) = sqlx::query(sql)
        .bind(&auth_user.user.id)
        .bind(&event_type_id)
        .execute(&state.pool)
        .await
    {
        return internal_error_response("save meeting favorite", &e);
    }
    axum::Json(serde_json::json!({"favorite": form.favorite})).into_response()
}
