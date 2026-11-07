//! Host notes on a booking (#173): free text the booking's hosts keep about
//! it, apart from what the guest typed (`bookings.notes`).
//!
//! Who reads and edits them is decided in one place, `note_booking`, which
//! both the page and its POST go through:
//!
//! - the owner of the event type (the account it is stored under; for a team
//!   event type, its creator): they see the booking on their bookings page and
//!   approve, cancel or reschedule it;
//! - the member a round-robin booking is assigned to: they run the meeting;
//! - every member of a collective booking (no assigned member) except those
//!   excluded from the event type (weight 0, as `hosts.rs` reads it): they all
//!   attend;
//! - the watcher who claimed the booking: they take it over.
//!
//! These are the hosts the overview lists the booking for (`HOST_SCOPE` in
//! `overview_stats.rs`), plus the owner. Nobody else: not a team admin who is
//! none of these (team admins manage the event type, not the meetings of
//! other members, and no page shows them those bookings), not the other
//! members of a round-robin team, not a watcher who was only offered the
//! booking, and not an instance admin as such (impersonation shows them what
//! the user sees, nothing more).
//!
//! Notes reach host surfaces only: this page, the bookings page and the
//! overview, and the host emails sent once they exist (see
//! `crate::email::booking_host_notes`). They are never shown to the guest,
//! written into an invite, an ICS file or the calendar write-back (the event
//! on the host's calendar is sent to its attendees by scheduling servers and
//! is often shared), an SMS or a webhook.

use super::*;

/// Longest note accepted, in characters, as for the guest's own notes.
pub(super) const HOST_NOTES_MAX_CHARS: usize = 5000;

/// Whether user `?` hosts booking `b` as described in the module
/// documentation. Aliases `b`, `et` and `a` (LEFT JOIN of the event type's
/// account). Binds the user id four times.
const NOTE_HOSTS: &str = "(a.user_id = ?
    OR b.assigned_user_id = ?
    OR (b.assigned_user_id IS NULL AND et.team_id IS NOT NULL AND EXISTS (
          SELECT 1 FROM team_members tm
          WHERE tm.team_id = et.team_id AND tm.user_id = ?
            AND NOT EXISTS (SELECT 1 FROM event_type_member_weights etw
                WHERE etw.event_type_id = et.id AND etw.user_id = tm.user_id
                  AND etw.weight <= 0)))
    OR b.claimed_by_user_id = ?)";

/// A booking as its notes page shows it.
pub(super) struct NoteBooking {
    pub id: String,
    pub status: String,
    pub event_title: String,
    pub guest_name: String,
    pub guest_email: String,
    pub guest_notes: String,
    pub start_at: String,
    pub end_at: String,
    pub stored_tz: String,
    pub guest_tz: String,
    pub host_notes: Option<String>,
    /// `host_notes_updated_at`, empty when the notes were never saved: the
    /// version a save is made against.
    pub updated_at: String,
    /// Who saved them last, `None` when nobody did or the user is gone.
    pub updated_by: Option<String>,
}

/// The booking `booking_id` when `user_id` may read and edit its host notes,
/// `None` otherwise (also when it does not exist: the two are not told
/// apart). The one authorization check of this module.
pub(super) async fn note_booking(
    pool: &SqlitePool,
    booking_id: &str,
    user_id: &str,
) -> Result<Option<NoteBooking>, sqlx::Error> {
    let sql = format!(
        "SELECT b.id, b.status, et.title, b.guest_name, b.guest_email, COALESCE(b.notes, ''),
                CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END,
                CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END,
                COALESCE(NULLIF(et.timezone, ''), u.timezone, 'UTC'),
                COALESCE(NULLIF(b.guest_timezone, ''), 'UTC'),
                b.host_notes, COALESCE(b.host_notes_updated_at, ''), eu.name
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         LEFT JOIN users eu ON eu.id = b.host_notes_updated_by
         WHERE b.id = ? AND {NOTE_HOSTS}"
    );
    #[allow(clippy::type_complexity)]
    let row: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
    )> = sqlx::query_as(&sql)
        .bind(booking_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(
        |(
            id,
            status,
            event_title,
            guest_name,
            guest_email,
            guest_notes,
            start_at,
            end_at,
            stored_tz,
            guest_tz,
            host_notes,
            updated_at,
            updated_by,
        )| NoteBooking {
            id,
            status,
            event_title,
            guest_name,
            guest_email,
            guest_notes,
            start_at,
            end_at,
            stored_tz,
            guest_tz,
            host_notes,
            updated_at,
            updated_by,
        },
    ))
}

/// What a host submitted, as it is stored: line endings normalised, outer
/// whitespace trimmed, `None` when nothing is left.
pub(super) fn normalize_host_notes(raw: &str) -> Option<String> {
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Where the page's back link goes: the bookings page by default, the
/// overview when the page was opened from there. Only these two, so the
/// parameter can never send anyone elsewhere.
fn back_target(back: Option<&str>) -> (&'static str, &'static str, &'static str) {
    match back {
        Some("overview") => ("overview", "/dashboard", "nav-overview"),
        _ => ("bookings", "/dashboard/bookings", "nav-bookings"),
    }
}

#[derive(Deserialize)]
pub(super) struct NotesQuery {
    back: Option<String>,
    saved: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct NotesForm {
    _csrf: Option<String>,
    #[serde(default)]
    host_notes: String,
    /// `host_notes_updated_at` as the page showed it.
    #[serde(default)]
    base: String,
    back: Option<String>,
}

/// What the form shows besides the booking.
struct NotesView<'a> {
    text: &'a str,
    base: &'a str,
    back: Option<&'a str>,
    saved: bool,
    error: Option<String>,
    /// The version saved meanwhile by someone else, after a refused save.
    conflict: Option<&'a str>,
}

fn not_found() -> Response {
    (axum::http::StatusCode::NOT_FOUND, "").into_response()
}

/// "Last edited by Alice, 7 Nov 14:30" in the viewer's zone, empty when the
/// notes were never saved or are empty.
fn last_edit_label(booking: &NoteBooking, viewer_tz: Tz, lang: &str) -> String {
    if booking.host_notes.is_none() {
        return String::new();
    }
    let Ok(at) = NaiveDateTime::parse_from_str(&booking.updated_at, "%Y-%m-%d %H:%M:%S%.f") else {
        return String::new();
    };
    let local = viewer_tz.from_utc_datetime(&at).naive_local();
    let when = format!(
        "{} {}",
        crate::i18n::format_short_date(local.date(), lang),
        local.format("%H:%M")
    );
    let mut args = fluent_bundle::FluentArgs::new();
    args.set("when", when);
    match &booking.updated_by {
        Some(name) => {
            args.set("name", name.clone());
            crate::i18n::translate(lang, "host-notes-last-edit", Some(&args))
        }
        None => crate::i18n::translate(lang, "host-notes-last-edit-anon", Some(&args)),
    }
}

fn render_notes_page(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    booking: &NoteBooking,
    view: NotesView<'_>,
) -> Response {
    let lang = auth_user.lang;
    let tmpl = match state.templates.get_template("booking_notes.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };
    let viewer_tz = auth_user.user.timezone.parse::<Tz>().unwrap_or(Tz::UTC);
    let when = dashboard_when(
        &booking.start_at,
        &booking.end_at,
        &booking.stored_tz,
        viewer_tz.name(),
        &booking.guest_tz,
        lang,
    );
    let (status_mark, status_key) = match booking.status.as_str() {
        "confirmed" => ("c", "overview-ruler-booked"),
        "pending" => ("p", "overview-ruler-pending"),
        "declined" => ("x", "admin-st-declined"),
        _ => ("x", "admin-st-cancelled"),
    };
    let (back, back_url, back_key) = back_target(view.back);
    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(auth_user, "bookings"),
            lang => lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
            booking_id => booking.id,
            event_title => booking.event_title,
            guest_name => booking.guest_name,
            guest_email => booking.guest_email,
            guest_notes => booking.guest_notes.trim(),
            when => when,
            status_mark => status_mark,
            status_label => crate::i18n::translate(lang, status_key, None),
            host_notes => view.text,
            base => view.base,
            max_chars => HOST_NOTES_MAX_CHARS,
            last_edit => last_edit_label(booking, viewer_tz, lang),
            saved => view.saved,
            error => view.error.unwrap_or_default(),
            conflict_active => view.conflict.is_some(),
            conflict => view.conflict.unwrap_or(""),
            back => back,
            back_url => back_url,
            back_label => crate::i18n::translate(lang, back_key, None),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

/// `GET /dashboard/bookings/{id}/notes`.
pub(super) async fn booking_notes_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(booking_id): Path<String>,
    Query(query): Query<NotesQuery>,
) -> Response {
    let booking = match note_booking(&state.pool, &booking_id, &auth_user.user.id).await {
        Ok(Some(b)) => b,
        Ok(None) => return not_found(),
        Err(e) => return internal_error_response("load booking notes", &e),
    };
    let text = booking.host_notes.clone().unwrap_or_default();
    let view = NotesView {
        text: &text,
        base: &booking.updated_at,
        back: query.back.as_deref(),
        saved: query.saved.is_some(),
        error: None,
        conflict: None,
    };
    render_notes_page(&state, &auth_user, &booking, view)
}

/// `POST /dashboard/bookings/{id}/notes`: save the notes, or clear them
/// when the field is empty. Refused, with the submitted text kept, when it
/// is too long or when someone else saved since the page was loaded.
pub(super) async fn save_booking_notes(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(booking_id): Path<String>,
    Form(form): Form<NotesForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user_id = &auth_user.user.id;
    let booking = match note_booking(&state.pool, &booking_id, user_id).await {
        Ok(Some(b)) => b,
        Ok(None) => return not_found(),
        Err(e) => return internal_error_response("load booking notes", &e),
    };
    let (back, _, _) = back_target(form.back.as_deref());
    let notes = normalize_host_notes(&form.host_notes);

    if notes.as_deref().map_or(0, |n| n.chars().count()) > HOST_NOTES_MAX_CHARS {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("max", HOST_NOTES_MAX_CHARS);
        let view = NotesView {
            text: &form.host_notes,
            base: &form.base,
            back: Some(back),
            saved: false,
            error: Some(crate::i18n::translate(
                auth_user.lang,
                "host-notes-too-long",
                Some(&args),
            )),
            conflict: None,
        };
        return render_notes_page(&state, &auth_user, &booking, view);
    }

    let saved_url = format!(
        "/dashboard/bookings/{}/notes?saved=1&back={back}",
        booking.id
    );
    if notes == booking.host_notes {
        return Redirect::to(&saved_url).into_response();
    }

    // Guarded on the version the page was loaded with, so a save from a
    // stale page cannot silently replace what another host wrote meanwhile.
    let changed = sqlx::query(
        "UPDATE bookings SET host_notes = ?,
                host_notes_updated_at = strftime('%Y-%m-%d %H:%M:%f', 'now'),
                host_notes_updated_by = ?
         WHERE id = ? AND COALESCE(host_notes_updated_at, '') = ?",
    )
    .bind(&notes)
    .bind(user_id)
    .bind(&booking.id)
    .bind(&form.base)
    .execute(&state.pool)
    .await;
    match changed {
        Ok(r) if r.rows_affected() == 1 => {
            tracing::info!(booking_id = %booking.id, user_id = %user_id, cleared = notes.is_none(), "host notes saved");
            Redirect::to(&saved_url).into_response()
        }
        Ok(_) => {
            let current = booking.host_notes.clone().unwrap_or_default();
            let view = NotesView {
                text: &form.host_notes,
                base: &booking.updated_at,
                back: Some(back),
                saved: false,
                error: Some(crate::i18n::translate(
                    auth_user.lang,
                    "host-notes-conflict",
                    None,
                )),
                conflict: Some(&current),
            };
            render_notes_page(&state, &auth_user, &booking, view)
        }
        Err(e) => internal_error_response("save booking notes", &e),
    }
}
