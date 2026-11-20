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
//! - every member of a collective booking (a collective event type, no
//!   assigned member) except those excluded from the event type (weight 0, as
//!   `hosts.rs` reads it): they all attend. A round-robin booking that lost its
//!   assigned member (the user was deleted, or a legacy row) opens to no team
//!   member, only to the owner;
//! - the watcher who claimed the booking: they take it over.
//!
//! These are the hosts the overview lists the booking for (`HOST_SCOPE` in
//! `overview_stats.rs`), plus the owner. Nobody else: not a team admin who is
//! none of these (team admins manage the event type, not the meetings of
//! other members, and no page shows them those bookings), not the other
//! members of a round-robin team, not a watcher who was only offered the
//! booking, and not an instance admin as such (impersonation shows them what
//! the user sees, nothing more; a save they make records them as its author).
//!
//! Access follows the team as it is now, not as it was when the meeting was
//! booked: a member who left the team loses access to its collective
//! bookings, one who joined gains it, and an assignee or claimant keeps it
//! whatever the team. The owner and the assigned member are read from the
//! booking and its event type at each request too.
//!
//! Notes reach host surfaces only: this page, the bookings page and the
//! overview, and the host emails sent once they exist, each recipient checked
//! against the same rule (`host_notes_for_reader`). They are never shown to the guest,
//! written into an invite, an ICS file or the calendar write-back (the event
//! on the host's calendar is sent to its attendees by scheduling servers and
//! is often shared), an SMS or a webhook.

use super::*;

/// Longest note accepted, in characters, as for the guest's own notes.
pub(super) const HOST_NOTES_MAX_CHARS: usize = 5000;

/// Whether user `?` hosts booking `b` as described in the module
/// documentation. Aliases `b`, `et` and `a` (LEFT JOIN of the event type's
/// account). Binds the user id four times.
pub(super) const NOTE_HOSTS: &str = "(a.user_id = ?
    OR b.assigned_user_id = ?
    OR (b.assigned_user_id IS NULL AND et.team_id IS NOT NULL
        AND et.scheduling_mode = 'collective' AND EXISTS (
          SELECT 1 FROM team_members tm
          WHERE tm.team_id = et.team_id AND tm.user_id = ?
            AND NOT EXISTS (SELECT 1 FROM event_type_member_weights etw
                WHERE etw.event_type_id = et.id AND etw.user_id = tm.user_id
                  AND etw.weight <= 0)))
    OR b.claimed_by_user_id = ?)";

/// The host notes of the booking with iCal `uid`, for a host email to the
/// user `reader`: `None` when there are none, when `reader` is unknown (an
/// organizer snapshot whose user is gone) or may not read them under the rule
/// above. The organizer an email is addressed to is the one the calendar
/// event names, which may have left the team since; the note follows who may
/// read it now, not the calendar.
///
/// Read apart from `BookingDetails` and `CancellationDetails` on purpose and
/// passed to the host senders as their own argument: those structs also feed
/// the guest's emails, the ICS files, the calendar write-back and the SMS.
///
/// A failed lookup is logged at warn with the booking and the email goes out
/// without the note: the email (an approval, a cancellation, a reminder) is
/// worth more than the note, and a reminder sent twice is worse than one
/// without it.
pub(crate) async fn host_notes_for_reader(
    pool: &SqlitePool,
    uid: &str,
    reader: Option<&str>,
) -> Option<String> {
    let reader = reader?;
    match try_host_notes_for_reader(pool, uid, reader).await {
        Ok(notes) => notes,
        Err(e) => {
            tracing::warn!(booking = %uid, user_id = %reader, error = %e, "host notes lookup failed; host email sent without them");
            None
        }
    }
}

async fn try_host_notes_for_reader(
    pool: &SqlitePool,
    uid: &str,
    reader: &str,
) -> Result<Option<String>, sqlx::Error> {
    let sql = format!(
        "SELECT b.host_notes FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         LEFT JOIN accounts a ON a.id = et.account_id
         WHERE b.uid = ? AND {NOTE_HOSTS}"
    );
    let notes: Option<Option<String>> = sqlx::query_scalar(&sql)
        .bind(uid)
        .bind(reader)
        .bind(reader)
        .bind(reader)
        .bind(reader)
        .fetch_optional(pool)
        .await?;
    Ok(notes
        .flatten()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty()))
}

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
    /// `host_notes_version`: the version a save is made against.
    pub version: i64,
    /// `host_notes_updated_at`, empty when the notes were never saved.
    pub updated_at: String,
    /// Who saved them last, `None` when nobody did or the user is gone.
    pub updated_by: Option<String>,
    /// The host an admin was impersonating when they saved them last.
    pub updated_as: Option<String>,
    /// The guest's answers to the booking questions (#210).
    pub answers: Vec<crate::email::BookingAnswer>,
    /// The `utm_*` parameters the guest arrived with, if any.
    pub source: Option<Attribution>,
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
                b.host_notes, b.host_notes_version, COALESCE(b.host_notes_updated_at, ''),
                eu.name, ea.name
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         LEFT JOIN users eu ON eu.id = b.host_notes_updated_by
         LEFT JOIN users ea ON ea.id = b.host_notes_updated_as
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
        i64,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(&sql)
        .bind(booking_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    let booking = row.map(
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
            version,
            updated_at,
            updated_by,
            updated_as,
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
            version,
            updated_at,
            updated_by,
            updated_as,
            answers: Vec::new(),
            source: None,
        },
    );
    let Some(mut booking) = booking else {
        return Ok(None);
    };
    booking.answers = super::questions::answers_by_booking(pool, &[booking.id.as_str()])
        .await
        .remove(&booking.id)
        .unwrap_or_default();
    booking.source = super::attribution::by_booking(pool, &[booking.id.as_str()])
        .await
        .remove(&booking.id);
    Ok(Some(booking))
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
    /// `host_notes_version` as the page showed it.
    #[serde(default)]
    version: String,
    back: Option<String>,
}

/// What the form shows besides the booking.
struct NotesView<'a> {
    text: &'a str,
    version: i64,
    back: Option<&'a str>,
    saved: bool,
    error: Option<String>,
    /// After a refused save: the version saved meanwhile by someone else,
    /// `None` inside when they cleared the notes.
    conflict: Option<Option<&'a str>>,
}

/// "Last edited by Alice, 7 Nov 14:30" in the viewer's zone, empty when the
/// notes were never saved or are empty.
fn last_edit_label(booking: &NoteBooking, viewer_tz: Tz, lang: &str) -> String {
    if booking.host_notes.is_none() {
        return String::new();
    }
    let Some(at) = crate::booking_time::naive(&booking.updated_at) else {
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
    match (&booking.updated_by, &booking.updated_as) {
        (Some(name), Some(host)) => {
            args.set("name", name.clone());
            args.set("host", host.clone());
            crate::i18n::translate(lang, "host-notes-last-edit-as", Some(&args))
        }
        (Some(name), None) => {
            args.set("name", name.clone());
            crate::i18n::translate(lang, "host-notes-last-edit", Some(&args))
        }
        (None, _) => crate::i18n::translate(lang, "host-notes-last-edit-anon", Some(&args)),
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
            answers => super::questions::display_rows(
                &booking.answers,
                lang,
                crate::email::AnswerReader::Host,
            ),
            source => booking.source.as_ref().map(Attribution::pairs),
            when => when,
            status_mark => status_mark,
            status_label => crate::i18n::translate(lang, status_key, None),
            host_notes => view.text,
            version => view.version,
            max_chars => HOST_NOTES_MAX_CHARS,
            last_edit => last_edit_label(booking, viewer_tz, lang),
            saved => view.saved,
            error => view.error.unwrap_or_default(),
            conflict_active => view.conflict.is_some(),
            conflict => view.conflict.flatten().unwrap_or(""),
            back => back,
            back_url => back_url,
            back_label => crate::i18n::translate(lang, back_key, None),
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
    .into_response()
}

/// Store `notes` on the booking when its version is still `version` and
/// `reader` may still edit them, both checked in the write itself: a host who
/// lost access after the page was read (left the team, was unassigned) cannot
/// write. `author` is who is recorded, `acting_as` the host an admin was
/// impersonating. `Ok(false)` when nothing was written.
pub(super) async fn write_host_notes(
    pool: &SqlitePool,
    booking_id: &str,
    notes: Option<&str>,
    version: i64,
    reader: &str,
    author: &str,
    acting_as: Option<&str>,
) -> Result<bool, sqlx::Error> {
    let sql = format!(
        "UPDATE bookings SET host_notes = ?, host_notes_version = host_notes_version + 1,
                host_notes_updated_at = datetime('now'),
                host_notes_updated_by = ?, host_notes_updated_as = ?
         WHERE id = ? AND host_notes_version = ? AND EXISTS (
             SELECT 1 FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             LEFT JOIN accounts a ON a.id = et.account_id
             WHERE b.id = bookings.id AND {NOTE_HOSTS})"
    );
    let result = sqlx::query(&sql)
        .bind(notes)
        .bind(author)
        .bind(acting_as)
        .bind(booking_id)
        .bind(version)
        .bind(reader)
        .bind(reader)
        .bind(reader)
        .bind(reader)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
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
        Ok(None) => return dashboard_not_found(&state, &auth_user),
        Err(e) => return internal_error_response("load booking notes", &e),
    };
    let text = booking.host_notes.clone().unwrap_or_default();
    let view = NotesView {
        text: &text,
        version: booking.version,
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
        Ok(None) => return dashboard_not_found(&state, &auth_user),
        Err(e) => return internal_error_response("load booking notes", &e),
    };
    let (back, _, _) = back_target(form.back.as_deref());
    let notes = normalize_host_notes(&form.host_notes);
    // A missing or garbled version can match no row: refused as stale.
    let version = form.version.trim().parse::<i64>().unwrap_or(-1);

    if notes.as_deref().map_or(0, |n| n.chars().count()) > HOST_NOTES_MAX_CHARS {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("max", HOST_NOTES_MAX_CHARS);
        let view = NotesView {
            text: &form.host_notes,
            version,
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

    // An admin impersonating a host is recorded as the author, the host as
    // whom they acted beside it.
    let (author, acting_as) = match &auth_user.impersonation {
        Some(info) => (info.admin_id.as_str(), Some(user_id.as_str())),
        None => (user_id.as_str(), None),
    };
    // Guarded on the version the page was loaded with, so a save from a
    // stale page cannot silently replace what another host wrote meanwhile.
    let changed = write_host_notes(
        &state.pool,
        &booking.id,
        notes.as_deref(),
        version,
        user_id,
        author,
        acting_as,
    )
    .await;
    match changed {
        Ok(true) => {
            tracing::info!(booking_id = %booking.id, user_id = %author, acting_as = ?acting_as, cleared = notes.is_none(), "host notes saved");
            Redirect::to(&saved_url).into_response()
        }
        Ok(false) => {
            // Read the row again: the version and the text shown must be the
            // ones that refused this save, not those read before it. Access
            // lost meanwhile answers like any other refusal of access.
            let current = match note_booking(&state.pool, &booking_id, user_id).await {
                Ok(Some(b)) => b,
                Ok(None) => return dashboard_not_found(&state, &auth_user),
                Err(e) => return internal_error_response("load booking notes", &e),
            };
            let view = NotesView {
                text: &form.host_notes,
                version: current.version,
                back: Some(back),
                saved: false,
                error: Some(crate::i18n::translate(
                    auth_user.lang,
                    "host-notes-conflict",
                    None,
                )),
                conflict: Some(current.host_notes.as_deref()),
            };
            render_notes_page(&state, &auth_user, &current, view)
        }
        Err(e) => internal_error_response("save booking notes", &e),
    }
}
