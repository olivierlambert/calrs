//! Teams: listing, creation form, settings and avatars.

use super::*;

// --- Dashboard: Teams ---

pub(super) async fn dashboard_teams(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let is_admin = user.role == "admin";

    // For non-admin users, also fetch which teams they admin
    let teams: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        i64,
    )> = if is_admin {
        sqlx::query_as(
            "SELECT t.id, t.name, t.slug, t.description, t.visibility, t.avatar_path, t.invite_token,
                    (SELECT COUNT(*) FROM team_members tm WHERE tm.team_id = t.id) as member_count
             FROM teams t
             ORDER BY t.name",
        )
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT t.id, t.name, t.slug, t.description, t.visibility, t.avatar_path, t.invite_token,
                    (SELECT COUNT(*) FROM team_members tm WHERE tm.team_id = t.id) as member_count
             FROM teams t
             JOIN team_members tm2 ON tm2.team_id = t.id
             WHERE tm2.user_id = ?
             ORDER BY t.name",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    let member_team_ids: std::collections::HashSet<String> = if is_admin {
        // Global admins can manage all teams
        std::collections::HashSet::new()
    } else {
        let ids: Vec<(String,)> =
            sqlx::query_as("SELECT team_id FROM team_members WHERE user_id = ? AND role = 'admin'")
                .bind(&user.id)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
        ids.into_iter().map(|(id,)| id).collect()
    };

    let tmpl = match state.templates.get_template("dashboard_teams.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let teams_ctx: Vec<minijinja::Value> = teams
        .iter()
        .map(
            |(id, name, slug, description, visibility, avatar_path, invite_token, member_count)| {
                let initials = name
                    .split_whitespace()
                    .filter_map(|w| w.chars().next())
                    .take(2)
                    .collect::<String>()
                    .to_uppercase();
                let user_is_team_admin = is_admin || member_team_ids.contains(id);
                context! {
                    id => id,
                    name => name,
                    slug => slug,
                    description => description,
                    visibility => visibility,
                    has_avatar => avatar_path.is_some(),
                    initials => initials,
                    member_count => member_count,
                    is_team_admin => user_is_team_admin,
                    invite_token => invite_token,
                }
            },
        )
        .collect();

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "teams"),
            lang => auth_user.lang,
            teams => teams_ctx,
            is_admin => is_admin,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Team creation form ---

#[derive(Deserialize)]
pub(super) struct TeamForm {
    _csrf: Option<String>,
    name: String,
    slug: String,
    description: Option<String>,
    visibility: Option<String>,
    #[serde(default)]
    members: String,
    #[serde(default)]
    group_ids: String,
}

/// Split a comma-separated form field into a Vec of non-empty trimmed strings.
fn split_csv_ids(s: &str) -> Vec<String> {
    s.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub(super) async fn show_team_form(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
) -> impl IntoResponse {
    let user = &admin.user;

    // Fetch all enabled users
    let all_users: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, email, avatar_path FROM users WHERE enabled = 1 ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let users_ctx: Vec<minijinja::Value> = all_users
        .iter()
        .map(|(id, name, email, avatar_path)| {
            context! {
                id => id,
                name => name,
                email => email,
                is_self => id == &user.id,
                has_avatar => avatar_path.is_some(),
                initials => compute_initials(name),
            }
        })
        .collect();

    // Fetch OIDC groups with member details (for stacked avatars)
    let oidc_groups: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT g.id, g.name, COUNT(ug.user_id) as member_count \
         FROM groups g LEFT JOIN user_groups ug ON ug.group_id = g.id \
         GROUP BY g.id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let groups_ctx =
        build_groups_ctx(&state.pool, &oidc_groups, &std::collections::HashSet::new()).await;

    let tmpl = match state.templates.get_template("team_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    Html(
        tmpl.render(context! {
            lang => admin.lang,
            sidebar => admin_sidebar_context(user, "teams"),
            users => users_ctx,
            oidc_groups => if groups_ctx.is_empty() { None } else { Some(groups_ctx) },
            form_name => "",
            form_slug => "",
            form_description => "",
            form_visibility => "public",
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

pub(super) async fn create_team(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
    headers: HeaderMap,
    Form(form): Form<TeamForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &admin.user;

    let name = form.name.trim().to_string();
    let slug = form.slug.trim().to_lowercase();
    let description = form
        .description
        .as_deref()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    let visibility = form.visibility.as_deref().unwrap_or("public");

    // Validate
    if name.is_empty() || slug.is_empty() {
        return render_team_form_error(
            &state,
            user,
            admin.lang,
            &crate::i18n::translate(admin.lang, "form-error-team-name-slug-required", None),
            &form,
        )
        .await
        .into_response();
    }

    if name.len() > 255 {
        return render_team_form_error(
            &state,
            user,
            admin.lang,
            &crate::i18n::translate(admin.lang, "form-error-team-name-length", None),
            &form,
        )
        .await
        .into_response();
    }

    if let Some(ref d) = description {
        if d.len() > 5000 {
            return render_team_form_error(
                &state,
                user,
                admin.lang,
                &crate::i18n::translate(admin.lang, "form-error-team-description-length", None),
                &form,
            )
            .await
            .into_response();
        }
    }

    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return render_team_form_error(
            &state,
            user,
            admin.lang,
            &crate::i18n::translate(admin.lang, "form-error-slug-charset", None),
            &form,
        )
        .await
        .into_response();
    }

    // Check slug against reserved names
    const RESERVED_SLUGS: &[&str] = &["new", "settings", "admin", "api"];
    if RESERVED_SLUGS.contains(&slug.as_str()) {
        return render_team_form_error(
            &state,
            user,
            admin.lang,
            &crate::i18n::translate(admin.lang, "form-error-slug-reserved", None),
            &form,
        )
        .await
        .into_response();
    }

    // Check slug uniqueness
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM teams WHERE slug = ?")
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);
    if existing.is_some() {
        return render_team_form_error(
            &state,
            user,
            admin.lang,
            &crate::i18n::translate(admin.lang, "form-error-team-slug-taken", None),
            &form,
        )
        .await
        .into_response();
    }

    let team_id = uuid::Uuid::new_v4().to_string();
    let invite_token = if visibility == "private" {
        Some(uuid::Uuid::new_v4().to_string())
    } else {
        None
    };

    // Insert team
    let _ = sqlx::query(
        "INSERT INTO teams (id, name, slug, description, visibility, invite_token, created_by) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&team_id)
    .bind(&name)
    .bind(&slug)
    .bind(&description)
    .bind(visibility)
    .bind(&invite_token)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    // Add creator as admin member
    let _ = sqlx::query(
        "INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')",
    )
    .bind(&team_id)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    // Add selected members (skip creator, already added)
    let member_ids = split_csv_ids(&form.members);
    for member_id in &member_ids {
        if member_id == &user.id {
            continue;
        }
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')",
        )
        .bind(&team_id)
        .bind(member_id)
        .execute(&state.pool)
        .await;
    }

    // Link OIDC groups and add their members
    let group_ids = split_csv_ids(&form.group_ids);
    for group_id in &group_ids {
        let _ = sqlx::query("INSERT OR IGNORE INTO team_groups (team_id, group_id) VALUES (?, ?)")
            .bind(&team_id)
            .bind(group_id)
            .execute(&state.pool)
            .await;

        // Add group members to team
        let group_members: Vec<(String,)> =
            sqlx::query_as("SELECT user_id FROM user_groups WHERE group_id = ?")
                .bind(group_id)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();

        for (member_user_id,) in &group_members {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'group')",
            )
            .bind(&team_id)
            .bind(member_user_id)
            .execute(&state.pool)
            .await;
        }
    }

    tracing::info!(team_id = %team_id, name = %name, slug = %slug, "Team created");

    Redirect::to("/dashboard/teams").into_response()
}

async fn render_team_form_error(
    state: &AppState,
    user: &crate::models::User,
    lang: &str,
    error: &str,
    form: &TeamForm,
) -> Html<String> {
    let all_users: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, email, avatar_path FROM users WHERE enabled = 1 ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let users_ctx: Vec<minijinja::Value> = all_users
        .iter()
        .map(|(id, name, email, avatar_path)| {
            context! {
                id => id,
                name => name,
                email => email,
                is_self => id == &user.id,
                has_avatar => avatar_path.is_some(),
                initials => compute_initials(name),
            }
        })
        .collect();

    let oidc_groups: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT g.id, g.name, COUNT(ug.user_id) as member_count \
         FROM groups g LEFT JOIN user_groups ug ON ug.group_id = g.id \
         GROUP BY g.id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let groups_ctx =
        build_groups_ctx(&state.pool, &oidc_groups, &std::collections::HashSet::new()).await;

    let tmpl = match state.templates.get_template("team_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    Html(
        tmpl.render(context! {
            lang => lang,
            sidebar => admin_sidebar_context(user, "teams"),
            users => users_ctx,
            oidc_groups => if groups_ctx.is_empty() { None } else { Some(groups_ctx) },
            form_name => &form.name,
            form_slug => &form.slug,
            form_description => form.description.as_deref().unwrap_or(""),
            form_visibility => form.visibility.as_deref().unwrap_or("public"),
            error => error,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Team settings & avatar ---

pub(super) async fn is_team_member(pool: &SqlitePool, user_id: &str, team_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM team_members WHERE user_id = ? AND team_id = ?",
    )
    .bind(user_id)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// Check if a user has team-admin role for a specific team.
pub(super) async fn is_team_admin(pool: &SqlitePool, user_id: &str, team_id: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM team_members WHERE user_id = ? AND team_id = ? AND role = 'admin'",
    )
    .bind(user_id)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

#[derive(Deserialize)]
pub(super) struct GroupSettingsForm {
    _csrf: Option<String>,
    name: Option<String>,
    slug: Option<String>,
    description: Option<String>,
    visibility: Option<String>,
    #[serde(default)]
    members: String,
    #[serde(default)]
    group_ids: String,
    #[serde(default)]
    admin_members: String,
}

pub(super) async fn team_settings_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(team_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Html(crate::i18n::translate(
            auth_user.lang,
            "error-team-not-found-or-not-admin",
            None,
        ));
    }

    let team: Option<(String, String, String, Option<String>, Option<String>, String, Option<String>)> =
        sqlx::query_as("SELECT id, name, slug, description, avatar_path, visibility, invite_token FROM teams WHERE id = ?")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let (tid, team_name, team_slug, description, avatar_path, visibility, invite_token) = match team
    {
        Some(t) => t,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-team-not-found",
                None,
            ))
        }
    };

    let members: Vec<(String, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT u.id, u.name, u.avatar_path, tm.role, tm.source FROM users u \
         JOIN team_members tm ON tm.user_id = u.id \
         WHERE tm.team_id = ? AND u.enabled = 1 \
         ORDER BY u.name",
    )
    .bind(&tid)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let members_ctx: Vec<minijinja::Value> = members
        .iter()
        .map(|(id, name, ap, role, source)| {
            context! {
                id => id,
                name => name,
                has_avatar => ap.is_some(),
                initials => compute_initials(name),
                role => role,
                source => source,
            }
        })
        .collect();

    // All enabled users for the member picker
    let all_users: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, email, avatar_path FROM users WHERE enabled = 1 ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let member_ids: std::collections::HashSet<&str> =
        members.iter().map(|(id, _, _, _, _)| id.as_str()).collect();

    let all_users_ctx: Vec<minijinja::Value> = all_users
        .iter()
        .map(|(id, name, email, avatar_path)| {
            context! {
                id => id,
                name => name,
                email => email,
                has_avatar => avatar_path.is_some(),
                initials => compute_initials(name),
                is_member => member_ids.contains(id.as_str()),
            }
        })
        .collect();

    // OIDC groups with member counts + linked status
    let oidc_groups: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT g.id, g.name, COUNT(ug.user_id) as member_count \
         FROM groups g LEFT JOIN user_groups ug ON ug.group_id = g.id \
         GROUP BY g.id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let linked_group_ids: Vec<(String,)> =
        sqlx::query_as("SELECT group_id FROM team_groups WHERE team_id = ?")
            .bind(&tid)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
    let linked_set: std::collections::HashSet<String> =
        linked_group_ids.into_iter().map(|(id,)| id).collect();

    let groups_ctx = build_groups_ctx(&state.pool, &oidc_groups, &linked_set).await;

    let linked_groups_only: Vec<(String, String, i64)> = oidc_groups
        .iter()
        .filter(|(id, _, _)| linked_set.contains(id))
        .cloned()
        .collect();
    let linked_groups_ctx = build_groups_ctx(
        &state.pool,
        &linked_groups_only,
        &std::collections::HashSet::new(),
    )
    .await;

    let tmpl = match state.templates.get_template("team_settings.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "teams"),
            lang => auth_user.lang,
            team_id => tid,
            team_name => team_name,
            team_slug => team_slug,
            team_description => description.unwrap_or_default(),
            team_has_avatar => avatar_path.is_some(),
            upload_error => avatar_upload_error(&query),
            avatar_max_bytes => AVATAR_MAX_BYTES,
            team_initials => compute_initials(&team_name),
            visibility => visibility,
            invite_token => invite_token,
            members => members_ctx,
            all_users => all_users_ctx,
            oidc_groups => if groups_ctx.is_empty() { None } else { Some(groups_ctx) },
            linked_groups => linked_groups_ctx,
            success => query
                .get("success")
                .map(|_| crate::i18n::translate(auth_user.lang, "settings-saved", None)),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

pub(super) async fn team_settings_save(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    Form(form): Form<GroupSettingsForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Redirect::to("/dashboard/event-types").into_response();
    }
    // Update name if provided
    if let Some(ref name) = form.name {
        let name = name.trim().chars().take(255).collect::<String>();
        if !name.is_empty() {
            let _ = sqlx::query("UPDATE teams SET name = ? WHERE id = ?")
                .bind(&name)
                .bind(&team_id)
                .execute(&state.pool)
                .await;
        }
    }

    // Update slug if provided (validate format and uniqueness)
    if let Some(ref slug) = form.slug {
        let slug = slug
            .trim()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .take(100)
            .collect::<String>();
        if !slug.is_empty() {
            let conflict: Option<(String,)> =
                sqlx::query_as("SELECT id FROM teams WHERE slug = ? AND id != ?")
                    .bind(&slug)
                    .bind(&team_id)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);
            if conflict.is_none() {
                let _ = sqlx::query("UPDATE teams SET slug = ? WHERE id = ?")
                    .bind(&slug)
                    .bind(&team_id)
                    .execute(&state.pool)
                    .await;
            }
        }
    }

    let desc = form
        .description
        .as_deref()
        .unwrap_or("")
        .trim()
        .chars()
        .take(5000)
        .collect::<String>();
    let desc = if desc.is_empty() { None } else { Some(desc) };
    let _ = sqlx::query("UPDATE teams SET description = ? WHERE id = ?")
        .bind(&desc)
        .bind(&team_id)
        .execute(&state.pool)
        .await;

    // Update visibility if provided
    if let Some(ref vis) = form.visibility {
        let vis = vis.trim();
        if vis == "public" || vis == "private" {
            let _ = sqlx::query("UPDATE teams SET visibility = ? WHERE id = ?")
                .bind(vis)
                .bind(&team_id)
                .execute(&state.pool)
                .await;
            // Generate invite token when switching to private (if none exists)
            if vis == "private" {
                let existing: Option<(Option<String>,)> =
                    sqlx::query_as("SELECT invite_token FROM teams WHERE id = ?")
                        .bind(&team_id)
                        .fetch_optional(&state.pool)
                        .await
                        .unwrap_or(None);
                if existing.map(|(t,)| t.is_none()).unwrap_or(true) {
                    let token = uuid::Uuid::new_v4().to_string();
                    let _ = sqlx::query("UPDATE teams SET invite_token = ? WHERE id = ?")
                        .bind(&token)
                        .bind(&team_id)
                        .execute(&state.pool)
                        .await;
                }
            }
        }
    }

    // Sync direct members (preserve group-synced members).
    // The hidden input is always present (populated by JS). An empty value means
    // either "remove all direct members" (valid for global admins) or JS failure.
    // We proceed either way — the safety net below re-adds non-admin users.
    let member_ids = split_csv_ids(&form.members);
    // 1. Remove direct members not in the submitted list
    let _ = sqlx::query(
        "DELETE FROM team_members WHERE team_id = ? AND source = 'direct' AND user_id NOT IN \
         (SELECT value FROM json_each(?))",
    )
    .bind(&team_id)
    .bind(serde_json::to_string(&member_ids).unwrap_or_else(|_| "[]".to_string()))
    .execute(&state.pool)
    .await;

    // 2. Add new direct members (INSERT OR IGNORE to not conflict with group-synced)
    for member_id in &member_ids {
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'direct')",
        )
        .bind(&team_id)
        .bind(member_id)
        .execute(&state.pool)
        .await;
    }

    // 3. Update member roles based on admin_members list
    let admin_ids: std::collections::HashSet<String> =
        split_csv_ids(&form.admin_members).into_iter().collect();
    // Set all members to 'member' first, then promote admins
    let _ = sqlx::query("UPDATE team_members SET role = 'member' WHERE team_id = ?")
        .bind(&team_id)
        .execute(&state.pool)
        .await;
    for admin_id in &admin_ids {
        let _ =
            sqlx::query("UPDATE team_members SET role = 'admin' WHERE team_id = ? AND user_id = ?")
                .bind(&team_id)
                .bind(admin_id)
                .execute(&state.pool)
                .await;
    }

    // Ensure at least one admin remains. Global admins can remove themselves
    // (they can still manage the team via the admin panel).
    if !is_admin {
        // Re-ensure the current user stays as admin
        let _ =
            sqlx::query("UPDATE team_members SET role = 'admin' WHERE team_id = ? AND user_id = ?")
                .bind(&team_id)
                .bind(&user.id)
                .execute(&state.pool)
                .await;
        // Also ensure they remain a member (INSERT OR IGNORE in case they were removed)
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')",
        )
        .bind(&team_id)
        .bind(&user.id)
        .execute(&state.pool)
        .await;
    }

    // Sync linked OIDC groups
    let group_ids = split_csv_ids(&form.group_ids);
    // 1. Remove unlinked groups
    let _ = sqlx::query(
        "DELETE FROM team_groups WHERE team_id = ? AND group_id NOT IN \
         (SELECT value FROM json_each(?))",
    )
    .bind(&team_id)
    .bind(serde_json::to_string(&group_ids).unwrap_or_else(|_| "[]".to_string()))
    .execute(&state.pool)
    .await;

    // 2. Add newly linked groups and their members
    for gid in &group_ids {
        let _ = sqlx::query("INSERT OR IGNORE INTO team_groups (team_id, group_id) VALUES (?, ?)")
            .bind(&team_id)
            .bind(gid)
            .execute(&state.pool)
            .await;

        // Add group members as team members with source='group'
        let group_members: Vec<(String,)> =
            sqlx::query_as("SELECT user_id FROM user_groups WHERE group_id = ?")
                .bind(gid)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
        for (uid,) in &group_members {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'member', 'group')",
            )
            .bind(&team_id)
            .bind(uid)
            .execute(&state.pool)
            .await;
        }
    }

    // 3. Remove group-sourced members whose groups are no longer linked
    let _ = sqlx::query(
        "DELETE FROM team_members WHERE team_id = ? AND source = 'group' \
         AND user_id NOT IN (SELECT ug.user_id FROM user_groups ug \
         JOIN team_groups tg ON tg.group_id = ug.group_id WHERE tg.team_id = ?)",
    )
    .bind(&team_id)
    .bind(&team_id)
    .execute(&state.pool)
    .await;

    tracing::info!(team_id = %team_id, user_id = %user.id, "team settings updated");
    Redirect::to(&format!("/dashboard/teams/{}/settings?success=1", team_id)).into_response()
}

pub(super) async fn upload_team_avatar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    Query(csrf_query): Query<CsrfQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf_query._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Redirect::to("/dashboard/event-types").into_response();
    }
    let redirect_url = format!("/dashboard/teams/{}/settings", team_id);
    let refused = |reason: &str| {
        Redirect::to(&format!("{redirect_url}?upload_error={reason}")).into_response()
    };
    // A body over the size limit makes the multipart reader fail, which ends
    // this loop and lands on the refusal below like any other bad upload.
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() != Some("avatar") {
            continue;
        }
        let Ok(bytes) = field.bytes().await else {
            return refused("avatar");
        };
        if bytes.len() > AVATAR_MAX_BYTES {
            return refused("avatar");
        }
        // Trust magic bytes, not the multipart Content-Type header.
        let Some(ext) = detect_image_ext(&bytes) else {
            return refused("avatar");
        };
        let avatars_dir = state.data_dir.join("avatars");
        let filename = format!("team_{}.{}", team_id, ext);
        let avatar_path = avatars_dir.join(&filename);
        let old: Option<(String,)> = sqlx::query_as(
            "SELECT avatar_path FROM teams WHERE id = ? AND avatar_path IS NOT NULL",
        )
        .bind(&team_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);
        if let Err(e) = write_avatar(&avatars_dir, &avatar_path, &bytes).await {
            tracing::error!(error = %e, team_id = %team_id, "failed to save the team avatar");
            return refused("avatar-save");
        }
        if let Err(e) = sqlx::query("UPDATE teams SET avatar_path = ? WHERE id = ?")
            .bind(&filename)
            .bind(&team_id)
            .execute(&state.pool)
            .await
        {
            tracing::error!(error = %e, team_id = %team_id, "failed to record the team avatar");
            return refused("avatar-save");
        }
        // Remove the old avatar once the new one is recorded, if its
        // extension differs (the same one was just overwritten).
        if let Some((old_name,)) = old {
            let old_full = avatars_dir.join(&old_name);
            if old_full != avatar_path {
                let _ = tokio::fs::remove_file(&old_full).await;
            }
        }
        tracing::info!(team_id = %team_id, user_id = %user.id, "team avatar uploaded");
        return Redirect::to(&redirect_url).into_response();
    }
    refused("avatar")
}

pub(super) async fn delete_team_avatar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Redirect::to("/dashboard/event-types").into_response();
    }
    let old: Option<(String,)> =
        sqlx::query_as("SELECT avatar_path FROM teams WHERE id = ? AND avatar_path IS NOT NULL")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    if let Some((avatar_path,)) = old {
        let full_path = state.data_dir.join("avatars").join(&avatar_path);
        let _ = tokio::fs::remove_file(&full_path).await;
    }
    let _ = sqlx::query("UPDATE teams SET avatar_path = NULL WHERE id = ?")
        .bind(&team_id)
        .execute(&state.pool)
        .await;
    tracing::info!(team_id = %team_id, user_id = %user.id, "team avatar deleted");
    Redirect::to(&format!("/dashboard/teams/{}/settings", team_id)).into_response()
}

pub(super) async fn delete_team(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Redirect::to("/dashboard/teams").into_response();
    }
    // Delete avatar file if present
    let old: Option<(String,)> =
        sqlx::query_as("SELECT avatar_path FROM teams WHERE id = ? AND avatar_path IS NOT NULL")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    if let Some((avatar_path,)) = old {
        let full_path = state.data_dir.join("avatars").join(&avatar_path);
        let _ = tokio::fs::remove_file(&full_path).await;
    }
    // Nullify team_id on event types (don't delete them — they belong to the creator's account)
    let _ = sqlx::query("UPDATE event_types SET team_id = NULL WHERE team_id = ?")
        .bind(&team_id)
        .execute(&state.pool)
        .await;
    // Delete team (CASCADE removes team_members, team_groups)
    let _ = sqlx::query("DELETE FROM teams WHERE id = ?")
        .bind(&team_id)
        .execute(&state.pool)
        .await;
    tracing::info!(team_id = %team_id, user_id = %user.id, "team deleted");
    Redirect::to("/dashboard/teams").into_response()
}

pub(super) async fn serve_team_avatar(
    State(state): State<Arc<AppState>>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    // Team avatars are always public (they're just logos/icons, not sensitive).
    let filename: Option<(String,)> =
        sqlx::query_as("SELECT avatar_path FROM teams WHERE id = ? AND avatar_path IS NOT NULL")
            .bind(&team_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let filename = match filename {
        Some((f,)) => f,
        None => return (axum::http::StatusCode::NOT_FOUND, "").into_response(),
    };

    let full_path = state.data_dir.join("avatars").join(&filename);
    match tokio::fs::read(&full_path).await {
        Ok(bytes) => {
            let content_type = if filename.ends_with(".jpg") || filename.ends_with(".jpeg") {
                "image/jpeg"
            } else if filename.ends_with(".png") {
                "image/png"
            } else if filename.ends_with(".gif") {
                "image/gif"
            } else if filename.ends_with(".webp") {
                "image/webp"
            } else {
                "image/png"
            };
            axum::response::Response::builder()
                .status(200)
                .header("Content-Type", content_type)
                .header("Cache-Control", "public, max-age=3600")
                .body(axum::body::Body::from(bytes))
                .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
                .into_response()
        }
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "").into_response(),
    }
}
