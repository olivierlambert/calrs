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
    #[serde(default)]
    favorites: bool,
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
    let search = query.q.trim().to_lowercase();
    entries.retain(|entry| {
        let text =
            format!("{} {} {}", entry.title, entry.owner_name, entry.owner_slug).to_lowercase();
        search.split_whitespace().all(|word| text.contains(word))
    });
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
