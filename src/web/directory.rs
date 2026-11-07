//! Discover shareable meeting types and keep personal dashboard favorites.

use super::*;

#[derive(sqlx::FromRow, serde::Serialize)]
pub(super) struct DirectoryEntry {
    pub(super) id: String,
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

/// Rows per page of the booking directory.
pub(super) const DIRECTORY_PAGE_SIZE: i64 = 50;
/// Favorites shown on the overview; past that the card links to the rest.
pub(super) const OVERVIEW_FAVORITES: i64 = 12;

/// The owner shown for a meeting: the person, or the team of a team meeting.
const OWNER_NAME: &str = "CASE WHEN et.team_id IS NULL THEN u.name ELSE t.name END";
const OWNER_SLUG: &str = "CASE WHEN et.team_id IS NULL THEN u.username ELSE t.slug END";

/// `pattern` matches a host of the team meeting `et`: an enabled member
/// not set to weight 0 for it (the rule round-robin and collective
/// scheduling use), on a public team or on one the viewer belongs to.
/// Members of a private team are only revealed to fellow members, since the
/// directory lists its internal meetings to every colleague.
fn push_host_match(
    qb: &mut sqlx::QueryBuilder<'_, sqlx::Sqlite>,
    viewer_id: &str,
    pattern: String,
) {
    qb.push(
        "(et.team_id IS NOT NULL
          AND (t.visibility = 'public'
               OR EXISTS (SELECT 1 FROM team_members vm
                          WHERE vm.team_id = et.team_id AND vm.user_id = ",
    );
    qb.push_bind(viewer_id.to_string());
    qb.push(
        ")) AND EXISTS (SELECT 1 FROM team_members tm JOIN users hu ON hu.id = tm.user_id
                       WHERE tm.team_id = et.team_id AND hu.enabled = 1
                         AND NOT EXISTS (SELECT 1 FROM event_type_member_weights w
                                         WHERE w.event_type_id = et.id AND w.user_id = hu.id
                                           AND w.weight <= 0)
                         AND (hu.name GLOB ",
    );
    qb.push_bind(pattern.clone());
    qb.push(" OR COALESCE(hu.username, '') GLOB ");
    qb.push_bind(pattern);
    qb.push(")))");
}

/// What to list. The visibility rules are not part of it: every query
/// applies them, so a favorite or an id never grants access by itself.
#[derive(Default)]
pub(super) struct DirectoryFilter<'a> {
    pub favorites_only: bool,
    pub event_type_id: Option<&'a str>,
    /// Words that must all match the title, the owner (person or team) or
    /// a host of a team meeting, literally and case blind.
    pub search: &'a str,
    /// At most this many rows, from `offset`. `None` lists everything.
    pub limit: Option<i64>,
    pub offset: i64,
}

pub(super) struct DirectoryPage {
    pub entries: Vec<DirectoryEntry>,
    /// Every row matching the filter, not only the page's.
    pub total: i64,
    /// Where the page starts, after clamping to the last page.
    pub offset: i64,
}

fn search_words(search: &str) -> Vec<String> {
    listing::bounded_search(search)
        .split_whitespace()
        .map(str::to_lowercase)
        .collect()
}

/// Listable meetings whose booking window has closed, by the date in their
/// own zone. A value is "well-formed" when `date(v, '+0 days')` gives it
/// back: the modifier makes SQLite normalise an impossible day such as
/// 2000-02-30, which plain `date()` returns unchanged in older versions. SQL already drops a well-formed `bookable_until` before
/// yesterday (closed in every zone) and keeps one from tomorrow on (open in
/// every zone), so only today's and yesterday's, plus any value SQLite does
/// not read as a date, are decided here, exactly as `BookingWindow` does.
async fn closed_window_ids(pool: &SqlitePool, utc_today: NaiveDate) -> Result<String, sqlx::Error> {
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.bookable_until, COALESCE(NULLIF(et.timezone, ''), u.timezone)
         FROM event_types et
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.enabled = 1 AND et.bookable_until IS NOT NULL
           AND NOT (date(et.bookable_until, '+0 days') IS et.bookable_until
                    AND (et.bookable_until < ? OR et.bookable_until > ?))",
    )
    .bind(
        (utc_today - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string(),
    )
    .bind(utc_today.format("%Y-%m-%d").to_string())
    .fetch_all(pool)
    .await?;
    let closed: Vec<String> = rows
        .into_iter()
        .filter(|(_, until, timezone)| {
            let tz = timezone
                .as_deref()
                .and_then(|s| s.parse::<Tz>().ok())
                .unwrap_or_else(server_tz);
            let window = BookingWindow {
                bookable_until: parse_optional_date(until.as_deref()),
                ..BookingWindow::default()
            };
            window.is_expired(today_in(tz))
        })
        .map(|(id, _, _)| id)
        .collect();
    Ok(serde_json::to_string(&closed).unwrap_or_else(|_| "[]".to_string()))
}

/// `FROM .. WHERE ..` of every directory query, with its parameters.
/// Use the same visibility rules for discovery, dashboard shortcuts and saves.
/// Internal types are explicitly shareable by any authenticated colleague,
/// including those on private teams. Public types on private teams require a
/// team token, so their bare URLs are not offered here. Private event types
/// remain in their owners' management pages.
fn push_listable<'a>(
    qb: &mut sqlx::QueryBuilder<'a, sqlx::Sqlite>,
    user_id: &str,
    filter: &DirectoryFilter<'_>,
    utc_today: NaiveDate,
    closed_ids: String,
) {
    qb.push(
        " FROM event_types et
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN event_type_favorites f ON f.event_type_id = et.id AND f.user_id = ",
    );
    qb.push_bind(user_id.to_string());
    qb.push(
        " WHERE et.enabled = 1 AND et.visibility IN ('public', 'internal')
           AND ((et.team_id IS NULL AND u.enabled = 1 AND COALESCE(u.username, '') <> '')
                OR (et.team_id IS NOT NULL AND COALESCE(t.slug, '') <> ''
                    AND (t.visibility = 'public' OR et.visibility = 'internal')))
           AND NOT (et.bookable_until IS NOT NULL
                    AND date(et.bookable_until, '+0 days') IS et.bookable_until
                    AND et.bookable_until < ",
    );
    qb.push_bind(
        (utc_today - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string(),
    );
    qb.push(") AND et.id NOT IN (SELECT value FROM json_each(");
    qb.push_bind(closed_ids);
    qb.push("))");
    if filter.favorites_only {
        qb.push(" AND f.user_id IS NOT NULL");
    }
    if let Some(id) = filter.event_type_id {
        qb.push(" AND et.id = ");
        qb.push_bind(id.to_string());
    }
    for word in search_words(filter.search) {
        let pattern = listing::ci_glob(&word);
        qb.push(" AND (COALESCE(et.title, '') GLOB ");
        qb.push_bind(pattern.clone());
        qb.push(format!(" OR COALESCE({OWNER_NAME}, '') GLOB "));
        qb.push_bind(pattern.clone());
        qb.push(format!(" OR COALESCE({OWNER_SLUG}, '') GLOB "));
        qb.push_bind(pattern.clone());
        qb.push(" OR ");
        push_host_match(qb, user_id, pattern);
        qb.push(")");
    }
}

/// The meetings the viewer may discover, filtered, ordered and paged in
/// SQL, so a large instance never loads its whole catalogue to show 50.
pub(super) async fn directory_page(
    pool: &SqlitePool,
    user_id: &str,
    filter: &DirectoryFilter<'_>,
) -> Result<DirectoryPage, sqlx::Error> {
    let utc_today = Utc::now().date_naive();
    let closed = closed_window_ids(pool, utc_today).await?;

    let mut count = sqlx::QueryBuilder::new("SELECT COUNT(*)");
    push_listable(&mut count, user_id, filter, utc_today, closed.clone());
    let total: i64 = count.build_query_scalar().fetch_one(pool).await?;
    if total == 0 {
        return Ok(DirectoryPage {
            entries: Vec::new(),
            total,
            offset: 0,
        });
    }
    // A page past the end (after an un-star or from a stale link) shows
    // the last page rather than an empty list.
    let offset = match filter.limit {
        Some(limit) if limit > 0 && filter.offset >= total => (total - 1) / limit * limit,
        _ => filter.offset.max(0),
    };

    let mut select = sqlx::QueryBuilder::new(format!(
        "SELECT et.id, et.title, et.duration_min, et.visibility,
                {OWNER_NAME} AS owner_name,
                {OWNER_SLUG} AS owner_slug,
                CASE WHEN et.team_id IS NULL THEN '/u/' || u.username || '/' || et.slug
                     ELSE '/team/' || t.slug || '/' || et.slug END AS url,
                f.user_id IS NOT NULL AS favorite,
                et.bookable_until, COALESCE(NULLIF(et.timezone, ''), u.timezone) AS timezone"
    ));
    push_listable(&mut select, user_id, filter, utc_today, closed);
    select.push(" ORDER BY owner_name COLLATE NOCASE, et.title COLLATE NOCASE, et.id");
    if let Some(limit) = filter.limit {
        select.push(" LIMIT ");
        select.push_bind(limit);
        select.push(" OFFSET ");
        select.push_bind(offset);
    }
    let mut entries: Vec<DirectoryEntry> = select.build_query_as().fetch_all(pool).await?;

    let words = search_words(filter.search);
    if !words.is_empty() {
        let hosts = matching_hosts(pool, user_id, &entries, &words).await?;
        for entry in &mut entries {
            entry.matched_hosts = matched_host_names(entry, &words, hosts.get(&entry.id));
        }
    }
    Ok(DirectoryPage {
        entries,
        total,
        offset,
    })
}

/// Whether the viewer may discover (and so star or invite to) one meeting.
pub(super) async fn is_listed(
    pool: &SqlitePool,
    user_id: &str,
    event_type_id: &str,
) -> Result<bool, sqlx::Error> {
    let filter = DirectoryFilter {
        event_type_id: Some(event_type_id),
        limit: Some(1),
        ..DirectoryFilter::default()
    };
    Ok(directory_page(pool, user_id, &filter).await?.total > 0)
}

/// Searchable hosts of the page's team meetings who match at least one
/// word, in name order. Bounded by the page, not by the size of the teams.
async fn matching_hosts(
    pool: &SqlitePool,
    viewer_id: &str,
    entries: &[DirectoryEntry],
    words: &[String],
) -> Result<HashMap<String, Vec<(String, String)>>, sqlx::Error> {
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT et.id, hu.name, COALESCE(hu.username, '')
         FROM event_types et
         JOIN teams t ON t.id = et.team_id
         JOIN team_members tm ON tm.team_id = et.team_id
         JOIN users hu ON hu.id = tm.user_id
         WHERE et.id IN (SELECT value FROM json_each(",
    );
    qb.push_bind(serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string()));
    qb.push(
        ")) AND hu.enabled = 1
           AND (t.visibility = 'public'
                OR EXISTS (SELECT 1 FROM team_members vm
                           WHERE vm.team_id = et.team_id AND vm.user_id = ",
    );
    qb.push_bind(viewer_id.to_string());
    qb.push(
        ")) AND NOT EXISTS (SELECT 1 FROM event_type_member_weights w
                           WHERE w.event_type_id = et.id AND w.user_id = hu.id AND w.weight <= 0)
           AND (0",
    );
    for word in words {
        let pattern = listing::ci_glob(word);
        qb.push(" OR hu.name GLOB ");
        qb.push_bind(pattern.clone());
        qb.push(" OR COALESCE(hu.username, '') GLOB ");
        qb.push_bind(pattern);
    }
    qb.push(") ORDER BY hu.name COLLATE NOCASE");
    let rows: Vec<(String, String, String)> = qb.build_query_as().fetch_all(pool).await?;
    let mut hosts: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (et_id, name, username) in rows {
        hosts.entry(et_id).or_default().push((name, username));
    }
    Ok(hosts)
}

/// Hosts matched by a word the meeting itself does not match, shown so a
/// team meeting found through a person does not look arbitrary. A match on
/// the team name alone lists nobody.
fn matched_host_names(
    entry: &DirectoryEntry,
    words: &[String],
    hosts: Option<&Vec<(String, String)>>,
) -> Vec<String> {
    let Some(hosts) = hosts else {
        return Vec::new();
    };
    let base = [&entry.title, &entry.owner_name, &entry.owner_slug];
    let host_only: Vec<&String> = words
        .iter()
        .filter(|w| !base.iter().any(|field| listing::ci_contains(field, w)))
        .collect();
    hosts
        .iter()
        .filter(|(name, username)| {
            host_only
                .iter()
                .any(|w| listing::ci_contains(name, w) || listing::ci_contains(username, w))
        })
        .map(|(name, _)| name.clone())
        .collect()
}

#[derive(Default, Deserialize)]
pub(super) struct DirectoryQuery {
    #[serde(default)]
    q: String,
    /// A hand-written `?favorites=1` or `=on` must not be a 400.
    #[serde(default, deserialize_with = "lenient_flag")]
    favorites: bool,
    /// Kept as text so `?page=abc` is the first page rather than a 400.
    #[serde(default)]
    page: Option<String>,
}

fn lenient_flag<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let value = String::deserialize(d)?;
    Ok(matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "on" | "yes"
    ))
}

/// Link to one page of the directory, keeping the search and the filter.
fn directory_href(search: &str, favorites_only: bool, page: i64) -> String {
    let mut params = Vec::new();
    if !search.is_empty() {
        params.push(format!("q={}", urlencoding::encode(search)));
    }
    if favorites_only {
        params.push("favorites=true".to_string());
    }
    if page > 1 {
        params.push(format!("page={page}"));
    }
    if params.is_empty() {
        "/dashboard/directory".to_string()
    } else {
        format!("/dashboard/directory?{}", params.join("&"))
    }
}

pub(super) async fn dashboard_directory(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<DirectoryQuery>,
) -> Response {
    let search = listing::bounded_search(&query.q);
    let requested = listing::lenient_page(query.page.as_deref());
    let filter = DirectoryFilter {
        favorites_only: query.favorites,
        search: &search,
        limit: Some(DIRECTORY_PAGE_SIZE),
        offset: requested
            .saturating_sub(1)
            .saturating_mul(DIRECTORY_PAGE_SIZE),
        ..DirectoryFilter::default()
    };
    let DirectoryPage {
        entries,
        total,
        offset,
    } = match directory_page(&state.pool, &auth_user.user.id, &filter).await {
        Ok(page) => page,
        Err(e) => return internal_error_response("load booking directory", &e),
    };
    let page = offset / DIRECTORY_PAGE_SIZE + 1;
    let pager = listing::pager(page, total, DIRECTORY_PAGE_SIZE, |n| {
        directory_href(&search, query.favorites, n)
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
        search => search,
        search_max => listing::MAX_SEARCH_CHARS,
        favorites_only => query.favorites,
        pager => pager,
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
        match is_listed(&state.pool, &auth_user.user.id, &event_type_id).await {
            Ok(false) => return axum::http::StatusCode::NOT_FOUND.into_response(),
            Err(e) => return internal_error_response("check favorite access", &e),
            Ok(true) => {}
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
