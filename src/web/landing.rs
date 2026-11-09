//! What the public landing pages (`/u/{username}`, `/team/{slug}`) show
//! besides the meetings themselves: the host-written sections and the
//! order of the page (with the dashboard page that arranges them), the
//! host's usual weekly hours, where they are based and the languages they
//! speak.

use super::*;
use fluent_bundle::{FluentArgs, FluentValue};

/// The longest section name kept, in characters.
pub(super) const SECTION_MAX_CHARS: usize = 80;
/// The longest "Based in" kept, in characters.
pub(super) const BASED_IN_MAX_CHARS: usize = 80;

/// The languages a host can say they speak, as BCP-47 codes: the ones calrs
/// ships, then common others. Each has a `language-name-{code}` message in
/// every locale, so the page names them in the visitor's language.
pub(super) const SPOKEN_LANGUAGES: [&str; 26] = [
    "en", "fr", "de", "es", "it", "pt", "nl", "pl", "et", "ar", "zh", "ja", "ko", "ru", "uk", "sv",
    "da", "nb", "fi", "cs", "ro", "el", "tr", "hi", "he", "hu",
];

/// A section name as stored: whitespace collapsed, control characters
/// dropped, at most [`SECTION_MAX_CHARS`] characters; `None` when nothing
/// is left. Shown as written otherwise.
pub(super) fn normalize_section(raw: &str) -> Option<String> {
    one_line(raw, SECTION_MAX_CHARS)
}

/// "Based in" as stored, the same way as a section name.
pub(super) fn normalize_based_in(raw: &str) -> Option<String> {
    one_line(raw, BASED_IN_MAX_CHARS)
}

fn one_line(raw: &str, max: usize) -> Option<String> {
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let joined: String = words.join(" ").chars().take(max).collect();
    let joined = joined.trim_end().to_string();
    (!joined.is_empty()).then_some(joined)
}

/// The spoken languages as stored: the known codes among `codes`, in the
/// order of [`SPOKEN_LANGUAGES`], joined with commas; `None` for none.
pub(super) fn normalize_spoken_languages(codes: &[String]) -> Option<String> {
    let picked: Vec<&str> = SPOKEN_LANGUAGES
        .iter()
        .copied()
        .filter(|code| codes.iter().any(|c| c.trim() == *code))
        .collect();
    (!picked.is_empty()).then(|| picked.join(","))
}

/// The stored codes, known ones only.
pub(super) fn spoken_codes(stored: Option<&str>) -> Vec<&'static str> {
    let stored = stored.unwrap_or("");
    SPOKEN_LANGUAGES
        .iter()
        .copied()
        .filter(|code| stored.split(',').any(|c| c.trim() == *code))
        .collect()
}

/// A language's name in `lang` ("French", "français").
pub(super) fn language_name(lang: &str, code: &str) -> String {
    crate::i18n::translate(lang, &format!("language-name-{code}"), None)
}

/// "A", "A and B", "A, B and C" in `lang`. Spanish writes "e" instead of
/// "y" before a word starting with an i sound ("francés e inglés").
pub(super) fn join_list(lang: &str, items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [head @ .., last] => {
            let first = head.join(", ");
            let lower = last.to_lowercase();
            let e = lower.starts_with('i')
                || lower.starts_with('í')
                || (lower.starts_with("hi")
                    && !lower.starts_with("hie")
                    && !lower.starts_with("hia"));
            let mut args = FluentArgs::new();
            args.set("first", FluentValue::from(first));
            args.set("last", FluentValue::from(last.clone()));
            args.set("e", FluentValue::from(if e { "yes" } else { "no" }));
            crate::i18n::translate(lang, "list-and", Some(&args))
        }
    }
}

/// The spoken languages, named in `lang` and joined into one phrase.
pub(super) fn spoken_phrase(lang: &str, stored: Option<&str>) -> Option<String> {
    let names: Vec<String> = spoken_codes(stored)
        .into_iter()
        .map(|code| language_name(lang, code))
        .collect();
    (!names.is_empty()).then(|| join_list(lang, &names))
}

// --- Sections and order ---

/// Meetings grouped for a landing page: the sections in the order of their
/// first meeting, each with its meetings in page order, then the meetings
/// without a section, under no name. `items` come in page order.
pub(super) fn group_by_section<T>(
    items: Vec<(Option<String>, T)>,
) -> Vec<(Option<String>, Vec<T>)> {
    let mut groups: Vec<(Option<String>, Vec<T>)> = Vec::new();
    let mut loose: Vec<T> = Vec::new();
    for (section, item) in items {
        match section.as_deref().and_then(normalize_section) {
            Some(name) => match groups
                .iter_mut()
                .find(|(label, _)| label.as_deref() == Some(name.as_str()))
            {
                Some((_, list)) => list.push(item),
                None => groups.push((Some(name), vec![item])),
            },
            None => loose.push(item),
        }
    }
    if !loose.is_empty() {
        groups.push((None, loose));
    }
    groups
}

/// The SQL order of a page: placed types by position, then the others in
/// creation order (how every page read before positions existed).
pub(super) const PAGE_ORDER: &str =
    "(et.page_position IS NULL), et.page_position, et.created_at, et.id";

/// What the arrange page moves, one step up or down.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Move {
    Type(String),
    Section(String),
}

/// The page order after `mv`, for `types` in page order as (id, section).
/// A type moves within its section (or among the types without one); a
/// section moves past its neighbour, the types without a section always
/// coming last. A move past either end changes nothing.
pub(super) fn apply_move(types: &[(String, Option<String>)], mv: &Move, up: bool) -> Vec<String> {
    let mut blocks = group_by_section(
        types
            .iter()
            .map(|(id, section)| (section.clone(), id.clone()))
            .collect(),
    );
    match mv {
        Move::Type(id) => {
            if let Some((_, list)) = blocks.iter_mut().find(|(_, l)| l.contains(id)) {
                let at = list.iter().position(|x| x == id).unwrap_or(0);
                if up && at > 0 {
                    list.swap(at, at - 1);
                } else if !up && at + 1 < list.len() {
                    list.swap(at, at + 1);
                }
            }
        }
        Move::Section(name) => {
            let named = blocks.iter().filter(|(label, _)| label.is_some()).count();
            if let Some(at) = blocks
                .iter()
                .position(|(label, _)| label.as_deref() == Some(name.as_str()))
            {
                if up && at > 0 {
                    blocks.swap(at, at - 1);
                } else if !up && at + 1 < named {
                    blocks.swap(at, at + 1);
                }
            }
        }
    }
    blocks.into_iter().flat_map(|(_, list)| list).collect()
}

/// Writes `section` (absent: left as stored) on an event type, in the
/// transaction that saves the rest of its form.
pub(super) async fn save_section_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    et_id: &str,
    section: Option<&str>,
) -> Result<(), sqlx::Error> {
    if let Some(raw) = section {
        sqlx::query("UPDATE event_types SET section = ? WHERE id = ?")
            .bind(normalize_section(raw))
            .bind(et_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// The section names offered when editing an event type: those already
/// used on the pages of the teams the user admins and on `team_id`'s (a
/// global admin editing a team's type), plus, with `personal`, on the
/// user's own page. A team's type gets team sections only.
pub(super) async fn section_suggestions(
    pool: &SqlitePool,
    user_id: &str,
    team_id: Option<&str>,
    personal: bool,
) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT DISTINCT et.section FROM event_types et
         LEFT JOIN accounts a ON a.id = et.account_id
         WHERE et.section IS NOT NULL AND et.section != ''
           AND ((? AND et.team_id IS NULL AND a.user_id = ?)
                OR et.team_id IN (SELECT team_id FROM team_members WHERE user_id = ? AND role = 'admin')
                OR (? IS NOT NULL AND et.team_id = ?))
         ORDER BY et.section COLLATE NOCASE
         LIMIT 100",
    )
    .bind(personal)
    .bind(user_id)
    .bind(user_id)
    .bind(team_id)
    .bind(team_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// The stored section of an event type, for its edit form.
pub(super) async fn stored_section(pool: &SqlitePool, et_id: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT section FROM event_types WHERE id = ?")
        .bind(et_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or_default()
}

// --- Usual hours ---

/// Weekdays Monday first, as `availability_rules.day_of_week` (0 = Sunday).
const WEEK: [u32; 7] = [1, 2, 3, 4, 5, 6, 0];

fn weekday_name(lang: &str, day: u32, short: bool) -> String {
    let key = match (day, short) {
        (0, false) => "common-weekday-long-sun",
        (1, false) => "common-weekday-long-mon",
        (2, false) => "common-weekday-long-tue",
        (3, false) => "common-weekday-long-wed",
        (4, false) => "common-weekday-long-thu",
        (5, false) => "common-weekday-long-fri",
        (_, false) => "common-weekday-long-sat",
        (0, true) => "slots-weekday-sun",
        (1, true) => "slots-weekday-mon",
        (2, true) => "slots-weekday-tue",
        (3, true) => "slots-weekday-wed",
        (4, true) => "slots-weekday-thu",
        (5, true) => "slots-weekday-fri",
        (_, true) => "slots-weekday-sat",
    };
    crate::i18n::translate(lang, key, None)
}

/// "Monday to Friday", "every day", "Monday, Wednesday and Friday": a run
/// of three days or more as a range, anything else as a list. A run may
/// wrap past the end of the week ("Friday to Monday"), which a visitor's
/// zone can make of a host's Saturday to Tuesday. With `short`, the
/// timetable's form: "Mon to Fri", "Mon, Wed and Fri".
fn days_phrase(lang: &str, days: &[u32], short: bool) -> String {
    if days.len() == 7 {
        return crate::i18n::translate(lang, "landing-days-every", None);
    }
    if let Some((first, last)) = circular_run(days) {
        let mut args = FluentArgs::new();
        args.set("first", FluentValue::from(weekday_name(lang, first, short)));
        args.set("last", FluentValue::from(weekday_name(lang, last, short)));
        let key = if short {
            "landing-days-span"
        } else {
            "landing-days-range"
        };
        return crate::i18n::translate(lang, key, Some(&args));
    }
    let sorted: Vec<u32> = WEEK.iter().copied().filter(|d| days.contains(d)).collect();
    let names: Vec<String> = sorted
        .iter()
        .map(|d| weekday_name(lang, *d, short))
        .collect();
    join_list(lang, &names)
}

/// The first and last day of `days` when they are three to six days in a
/// row of the Monday-first week, the run allowed to wrap past Sunday.
fn circular_run(days: &[u32]) -> Option<(u32, u32)> {
    let n = days.len();
    if !(3..7).contains(&n) {
        return None;
    }
    let has = |i: usize| days.contains(&WEEK[i % 7]);
    let start = (0..7).find(|&i| has(i) && !has(i + 6))?;
    (0..n)
        .all(|k| has(start + k))
        .then(|| (WEEK[start], WEEK[(start + n - 1) % 7]))
}

/// The place name of a timezone: "Europe/Paris" reads "Paris".
fn zone_place(tz: &str) -> String {
    tz.rsplit('/').next().unwrap_or(tz).replace('_', " ")
}

/// Days that share the same first and last minute, Monday first.
type HourGroup = (Vec<u32>, String, String);

/// The weekly hours of `rules` (day, "HH:MM", "HH:MM"), per open day its
/// first and last minute, days with the same hours grouped, groups in the
/// order of their first day (Monday first).
pub(super) fn hour_groups(rules: &[(i32, String, String)]) -> Vec<HourGroup> {
    let mut groups: Vec<HourGroup> = Vec::new();
    for day in WEEK {
        let windows = rules
            .iter()
            .filter(|(d, s, e)| *d as u32 == day && s < e && valid_hm(s) && valid_hm(e));
        let start = windows.clone().map(|(_, s, _)| s).min();
        let end = windows.map(|(_, _, e)| e).max();
        let (Some(start), Some(end)) = (start, end) else {
            continue;
        };
        match groups.iter_mut().find(|(_, s, e)| s == start && e == end) {
            Some((days, _, _)) => days.push(day),
            None => groups.push((vec![day], start.clone(), end.clone())),
        }
    }
    groups
}

fn valid_hm(s: &str) -> bool {
    chrono::NaiveTime::parse_from_str(s, "%H:%M").is_ok()
}

/// Distinct schedules beyond this many are not written out: the block
/// says the hours vary by day instead.
const TIMETABLE_MAX_ROWS: usize = 4;

/// The host's usual hours for a landing page, from the weekly rules of the
/// meetings it lists, all in timezone `tz`: a heading naming the timezone
/// once ("Usually available, Paris time") over a small timetable, one row
/// per distinct schedule with short day names ("Mon to Thu", "09:00 to
/// 17:30"), plus the groups behind the rows for the page script that
/// shows them in the visitor's timezone. Beyond [`TIMETABLE_MAX_ROWS`]
/// schedules, no rows: one sentence (`text`) says on which days the host
/// is available and that the hours vary by day. `None` without any rule.
pub(super) fn usual_hours(
    lang: &str,
    rules: &[(i32, String, String)],
    tz: &str,
) -> Option<minijinja::Value> {
    let groups = hour_groups(rules);
    let zone = zone_place(tz);
    let mut args = FluentArgs::new();
    args.set("zone", FluentValue::from(zone));
    if groups.is_empty() {
        return None;
    }
    let written = groups.len() <= TIMETABLE_MAX_ROWS;
    // Too many schedules for the timetable: one sentence instead.
    let text = if written {
        String::new()
    } else {
        let mut all: Vec<u32> = groups.iter().flat_map(|(d, _, _)| d.clone()).collect();
        all.sort_by_key(|d| WEEK.iter().position(|w| w == d));
        args.set("days", FluentValue::from(days_phrase(lang, &all, false)));
        crate::i18n::translate(lang, "landing-hours-vary", Some(&args))
    };
    let heading = crate::i18n::translate(lang, "landing-hours-heading", Some(&args));
    let range = crate::i18n::translate(lang, "landing-time-range", None);
    let rows: Vec<minijinja::Value> = if written {
        groups
            .iter()
            .map(|(days, start, end)| {
                context! {
                    days => days_phrase(lang, days, true),
                    hours => range.replacen("%1", start, 1).replacen("%2", end, 1),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    // The visitor's own hours only make sense for hours written out.
    let shown: Vec<minijinja::Value> = if written {
        groups
            .iter()
            .map(|(days, start, end)| context! { days => days, start => start, end => end })
            .collect()
    } else {
        Vec::new()
    };
    Some(context! { text => text, heading => heading, rows => rows, tz => tz, groups => shown })
}

/// Rules and timezone of the meetings `ids`, for [`usual_hours`]: `None`
/// when they do not share one timezone (the line would mislead) or have no
/// rule.
pub(super) async fn page_rules(
    pool: &SqlitePool,
    ids: &[&String],
) -> Option<(Vec<(i32, String, String)>, String)> {
    if ids.is_empty() {
        return None;
    }
    let ids_json = serde_json::to_string(ids).ok()?;
    let rows: Vec<(i32, String, String, String)> = sqlx::query_as(
        "SELECT ar.day_of_week, ar.start_time, ar.end_time,
                COALESCE(NULLIF(et.timezone, ''), u.timezone, 'UTC')
         FROM availability_rules ar
         JOIN event_types et ON et.id = ar.event_type_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.id IN (SELECT value FROM json_each(?))",
    )
    .bind(&ids_json)
    .fetch_all(pool)
    .await
    .ok()?;
    let tz = rows.first()?.3.clone();
    if rows.iter().any(|r| r.3 != tz) || tz.parse::<chrono_tz::Tz>().is_err() {
        return None;
    }
    Some((rows.into_iter().map(|(d, s, e, _)| (d, s, e)).collect(), tz))
}

/// The cards of a landing page: one per section in page order, then one for
/// the meetings without a section; a single card when no meeting has a
/// section. Each card: its title (the section as written, else the
/// translated "Meetings", or "Other meetings" after named sections), its
/// count and its meetings.
pub(super) fn landing_cards(
    lang: &str,
    items: Vec<(Option<String>, minijinja::Value)>,
) -> Vec<minijinja::Value> {
    let groups = group_by_section(items);
    let named = groups.iter().any(|(label, _)| label.is_some());
    groups
        .into_iter()
        .map(|(label, list)| {
            let title = match label {
                Some(name) => name,
                None if named => crate::i18n::translate(lang, "landing-other-meetings", None),
                None => crate::i18n::translate(lang, "landing-meetings", None),
            };
            context! { title => title, count => list.len(), items => list }
        })
        .collect()
}

// --- The arrange page ---

/// Whose page is being arranged.
enum Scope {
    Personal { user_id: String },
    Team { team_id: String },
}

async fn scope_types(
    pool: &SqlitePool,
    scope: &Scope,
) -> Vec<(String, String, Option<String>, bool, String)> {
    let order = PAGE_ORDER;
    match scope {
        Scope::Personal { user_id } => sqlx::query_as(&format!(
            "SELECT et.id, et.title, et.section, et.enabled = 1 AND et.visibility = 'public', et.visibility
             FROM event_types et JOIN accounts a ON a.id = et.account_id
             WHERE a.user_id = ? AND et.team_id IS NULL
             ORDER BY {order}"
        ))
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default(),
        Scope::Team { team_id } => sqlx::query_as(&format!(
            "SELECT et.id, et.title, et.section, et.enabled = 1 AND et.visibility = 'public', et.visibility
             FROM event_types et
             WHERE et.team_id = ?
             ORDER BY {order}"
        ))
        .bind(team_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default(),
    }
}

async fn arrange_render(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    scope: &Scope,
    title: &str,
    page_url: &str,
    action: &str,
    back: &str,
) -> Response {
    let rows = scope_types(&state.pool, scope).await;
    let items: Vec<(Option<String>, minijinja::Value)> = rows
        .iter()
        .map(|(id, title, section, listed, visibility)| {
            (
                section.clone(),
                context! { id => id, title => title, listed => listed, visibility => visibility },
            )
        })
        .collect();
    let groups: Vec<minijinja::Value> = group_by_section(items)
        .into_iter()
        .map(|(label, list)| context! { label => label, items => list })
        .collect();
    let tmpl = match state.templates.get_template("dashboard_page_order.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e).into_response(),
    };
    Html(
        tmpl.render(context! {
            lang => auth_user.lang,
            sidebar => sidebar_context(auth_user, "event-types"),
            heading => title,
            page_url => page_url,
            action => action,
            back => back,
            groups => groups,
            has_types => !rows.is_empty(),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct MoveForm {
    _csrf: Option<String>,
    /// "type" or "section".
    kind: String,
    /// The event type id, or the section name.
    target: String,
    /// "up" or "down".
    dir: String,
}

/// Applies a move to a page and writes every position of it in one
/// transaction.
async fn move_in_scope(pool: &SqlitePool, scope: &Scope, form: &MoveForm) -> bool {
    let mv = match form.kind.as_str() {
        "type" => Move::Type(form.target.clone()),
        "section" => Move::Section(form.target.clone()),
        _ => return false,
    };
    let up = match form.dir.as_str() {
        "up" => true,
        "down" => false,
        _ => return false,
    };
    let types: Vec<(String, Option<String>)> = scope_types(pool, scope)
        .await
        .into_iter()
        .map(|(id, _, section, _, _)| (id, section))
        .collect();
    let order = apply_move(&types, &mv, up);
    let Ok(mut tx) = pool.begin().await else {
        return false;
    };
    for (position, id) in order.iter().enumerate() {
        // Every id came from this page's own query: a move cannot reach
        // another page's types.
        if sqlx::query("UPDATE event_types SET page_position = ? WHERE id = ?")
            .bind(position as i64)
            .bind(id)
            .execute(&mut *tx)
            .await
            .is_err()
        {
            return false;
        }
    }
    tx.commit().await.is_ok()
}

/// `GET /dashboard/event-types/order`: the order of the user's own page.
pub(super) async fn personal_page_order(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let scope = Scope::Personal {
        user_id: auth_user.user.id.clone(),
    };
    let page = auth_user
        .user
        .username
        .as_deref()
        .map(|u| format!("/u/{u}"))
        .unwrap_or_default();
    let title = crate::i18n::translate(auth_user.lang, "page-order-heading", None);
    arrange_render(
        &state,
        &auth_user,
        &scope,
        &title,
        &page,
        "/dashboard/event-types/order",
        "/dashboard/event-types",
    )
    .await
}

/// `POST /dashboard/event-types/order`.
pub(super) async fn personal_page_order_move(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<MoveForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let scope = Scope::Personal {
        user_id: auth_user.user.id.clone(),
    };
    move_in_scope(&state.pool, &scope, &form).await;
    Redirect::to("/dashboard/event-types/order").into_response()
}

/// The team a user may arrange: a team admin's, or any for a global admin.
async fn arrangeable_team(
    pool: &SqlitePool,
    user: &crate::models::User,
    team_id: &str,
) -> Option<(String, String)> {
    let team: Option<(String, String)> =
        sqlx::query_as("SELECT name, slug FROM teams WHERE id = ?")
            .bind(team_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    let team = team?;
    (user.role == "admin" || is_team_admin(pool, &user.id, team_id).await).then_some(team)
}

/// `GET /dashboard/teams/{team_id}/order`: the order of a team's page.
pub(super) async fn team_page_order(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    let Some((name, slug)) = arrangeable_team(&state.pool, &auth_user.user, &team_id).await else {
        return (axum::http::StatusCode::FORBIDDEN, "Forbidden").into_response();
    };
    let scope = Scope::Team {
        team_id: team_id.clone(),
    };
    let title = tr1(auth_user.lang, "page-order-team-heading", "team", &name);
    arrange_render(
        &state,
        &auth_user,
        &scope,
        &title,
        &format!("/team/{slug}"),
        &format!("/dashboard/teams/{team_id}/order"),
        &format!("/dashboard/teams/{team_id}/settings"),
    )
    .await
}

/// `POST /dashboard/teams/{team_id}/order`.
pub(super) async fn team_page_order_move(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(team_id): Path<String>,
    Form(form): Form<MoveForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    if arrangeable_team(&state.pool, &auth_user.user, &team_id)
        .await
        .is_none()
    {
        return (axum::http::StatusCode::FORBIDDEN, "Forbidden").into_response();
    }
    let scope = Scope::Team {
        team_id: team_id.clone(),
    };
    move_in_scope(&state.pool, &scope, &form).await;
    Redirect::to(&format!("/dashboard/teams/{team_id}/order")).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sections_group_in_order_of_their_first_meeting() {
        let groups = group_by_section(vec![
            (Some("Working together".into()), "a"),
            (None, "b"),
            (Some(" Getting  started ".into()), "c"),
            (Some("Working together".into()), "d"),
            (Some("   ".into()), "e"),
        ]);
        assert_eq!(
            groups,
            vec![
                (Some("Working together".into()), vec!["a", "d"]),
                (Some("Getting started".into()), vec!["c"]),
                (None, vec!["b", "e"]),
            ]
        );
    }

    #[test]
    fn moves_stay_inside_their_section_and_sections_before_the_loose() {
        let page = vec![
            ("a".to_string(), Some("S1".to_string())),
            ("b".to_string(), Some("S1".to_string())),
            ("c".to_string(), Some("S2".to_string())),
            ("d".to_string(), None),
        ];
        assert_eq!(
            apply_move(&page, &Move::Type("b".into()), true),
            ids(&["b", "a", "c", "d"])
        );
        // The first of a section does not leave it.
        assert_eq!(
            apply_move(&page, &Move::Type("c".into()), true),
            ids(&["a", "b", "c", "d"])
        );
        assert_eq!(
            apply_move(&page, &Move::Section("S2".into()), true),
            ids(&["c", "a", "b", "d"])
        );
        // The last named section stays before the meetings without one.
        assert_eq!(
            apply_move(&page, &Move::Section("S2".into()), false),
            ids(&["a", "b", "c", "d"])
        );
        assert_eq!(
            apply_move(&page, &Move::Type("zz".into()), true),
            ids(&["a", "b", "c", "d"])
        );
    }

    #[test]
    fn section_names_are_one_trimmed_line() {
        assert_eq!(normalize_section("  a\n b\t"), Some("a b".to_string()));
        assert_eq!(normalize_section(" \u{7} "), None);
        assert_eq!(
            normalize_section(&"x".repeat(200)).unwrap().chars().count(),
            80
        );
    }

    fn rule(d: i32, s: &str, e: &str) -> (i32, String, String) {
        (d, s.to_string(), e.to_string())
    }

    /// The timetable rows of a `usual_hours` value, "days: hours".
    fn rows(v: &minijinja::Value) -> Vec<String> {
        v.get_attr("rows")
            .unwrap()
            .try_iter()
            .unwrap()
            .map(|r| {
                format!(
                    "{}: {}",
                    r.get_attr("days").unwrap(),
                    r.get_attr("hours").unwrap()
                )
            })
            .collect()
    }

    #[test]
    fn the_timetable_names_the_zone_once_and_one_row_per_schedule() {
        let mut week: Vec<_> = (1..=4).map(|d| rule(d, "09:00", "17:30")).collect();
        week.push(rule(5, "09:00", "16:00"));
        let v = usual_hours("en", &week, "Europe/Paris").unwrap();
        assert_eq!(
            v.get_attr("heading").unwrap().to_string(),
            "Usually available, Paris time"
        );
        assert_eq!(
            rows(&v),
            ["Mon to Thu: 09:00 to 17:30", "Fri: 09:00 to 16:00"]
        );
        let v = usual_hours("fr", &week, "Europe/Paris").unwrap();
        assert_eq!(
            rows(&v),
            ["Lun au Jeu: de 09:00 à 17:30", "Ven: de 09:00 à 16:00"]
        );
        let v = usual_hours("de", &week, "Europe/Berlin").unwrap();
        assert_eq!(rows(&v), ["Mo–Do: 09:00 bis 17:30", "Fr: 09:00 bis 16:00"]);
        let every: Vec<_> = (0..=6).map(|d| rule(d, "10:00", "16:00")).collect();
        let v = usual_hours("en", &every, "UTC").unwrap();
        assert_eq!(rows(&v), ["every day: 10:00 to 16:00"]);
        assert_eq!(days_phrase("en", &[1, 3, 5], true), "Mon, Wed and Fri");
        assert_eq!(days_phrase("en", &[5, 6, 0, 1], true), "Fri to Mon");
    }

    #[test]
    fn usual_hours_write_up_to_four_schedules_and_vary_beyond() {
        let week: Vec<_> = (1..=5).map(|d| rule(d, "09:00", "17:30")).collect();
        let v = usual_hours("en", &week, "Europe/Paris").unwrap();
        assert_eq!(rows(&v), ["Mon to Fri: 09:00 to 17:30"]);
        assert_eq!(v.get_attr("text").unwrap().to_string(), "");
        // A lunch break does not split the day.
        let mut split = week.clone();
        split.push(rule(1, "13:30", "17:30"));
        split[0] = rule(1, "09:00", "12:00");
        let v = usual_hours("en", &split, "Europe/Paris").unwrap();
        assert_eq!(rows(&v), ["Mon to Fri: 09:00 to 17:30"]);
        let odd = vec![
            rule(1, "08:00", "12:00"),
            rule(3, "10:00", "18:00"),
            rule(5, "14:00", "20:00"),
        ];
        let v = usual_hours("en", &odd, "Europe/Paris").unwrap();
        assert_eq!(v.get_attr("groups").unwrap().len(), Some(3));
        assert_eq!(
            rows(&v),
            [
                "Mon: 08:00 to 12:00",
                "Wed: 10:00 to 18:00",
                "Fri: 14:00 to 20:00"
            ]
        );
        // Five do not fit: the sentence alone.
        let five: Vec<_> = (1..=5)
            .map(|d| rule(d, &format!("0{d}:00"), "18:00"))
            .collect();
        let v = usual_hours("en", &five, "Europe/Paris").unwrap();
        assert_eq!(v.get_attr("rows").unwrap().len(), Some(0));
        assert_eq!(v.get_attr("groups").unwrap().len(), Some(0));
        assert_eq!(
            v.get_attr("text").unwrap().to_string(),
            "Usually available Monday to Friday (Paris time), hours vary by day"
        );
        assert!(usual_hours("en", &[], "Europe/Paris").is_none());
        assert!(usual_hours("en", &[rule(1, "18:00", "09:00")], "UTC").is_none());
    }

    #[test]
    fn day_runs_may_wrap_past_sunday() {
        assert_eq!(
            days_phrase("en", &[1, 2, 3, 4, 5], false),
            "Monday to Friday"
        );
        assert_eq!(
            days_phrase("en", &[0, 1, 2, 3, 4], false),
            "Sunday to Thursday"
        );
        assert_eq!(days_phrase("en", &[5, 6, 0, 1], false), "Friday to Monday");
        assert_eq!(days_phrase("en", &[6, 0], false), "Saturday and Sunday");
        assert_eq!(
            days_phrase("en", &[1, 3, 5], false),
            "Monday, Wednesday and Friday"
        );
        assert_eq!(
            days_phrase("en", &[0, 1, 2, 3, 4, 5, 6], false),
            "every day"
        );
    }

    #[test]
    fn spoken_languages_keep_known_codes_and_read_in_the_visitor_language() {
        let stored = normalize_spoken_languages(&ids(&["en", "xx", "fr", "fr"])).unwrap();
        assert_eq!(stored, "en,fr");
        assert_eq!(
            spoken_phrase("en", Some(&stored)).unwrap(),
            "English and French"
        );
        assert_eq!(
            spoken_phrase("fr", Some(&stored)).unwrap(),
            "anglais et français"
        );
        assert_eq!(
            spoken_phrase("es", Some("fr,en")).unwrap(),
            "inglés y francés"
        );
        assert_eq!(
            join_list("es", &ids(&["francés", "inglés"])),
            "francés e inglés"
        );
        assert_eq!(join_list("en", &ids(&["A", "B", "C"])), "A, B and C");
        assert!(spoken_phrase("en", Some("")).is_none());
        assert!(normalize_spoken_languages(&[]).is_none());
    }

    #[test]
    fn every_spoken_language_has_a_name_in_every_locale() {
        for (lang, _) in crate::i18n::supported_with_labels() {
            for code in SPOKEN_LANGUAGES {
                let key = format!("language-name-{code}");
                assert_ne!(
                    crate::i18n::translate(lang, &key, None),
                    key,
                    "{lang} {code}"
                );
            }
        }
    }
}
