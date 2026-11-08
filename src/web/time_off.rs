//! The time off page, `/dashboard/time-off` (#46): the signed-in user's own
//! periods, listed, added and deleted. The periods and the busy times they
//! make live in `crate::time_off`; every query here is scoped to the user.

use super::*;
use crate::time_off::{self, Period};

/// An admin viewing someone's dashboard as them gets the page read-only and
/// without the notes, which are the user's own (like MFA, which refuses
/// impersonation outright). A refused change is logged with the admin.
fn refuse_impersonated_change(auth_user: &crate::auth::AuthUser, action: &str) -> Option<Response> {
    let info = auth_user.impersonation.as_ref()?;
    tracing::warn!(
        admin = %info.admin_name,
        user_id = %auth_user.user.id,
        action,
        "time off change refused while impersonating"
    );
    Some(axum::http::StatusCode::FORBIDDEN.into_response())
}

#[derive(Deserialize, Default)]
pub(super) struct TimeOffForm {
    _csrf: Option<String>,
    #[serde(default)]
    start_date: String,
    #[serde(default)]
    start_time: String,
    #[serde(default)]
    end_date: String,
    #[serde(default)]
    end_time: String,
    #[serde(default)]
    label: String,
}

#[derive(Deserialize)]
pub(super) struct TimeOffQuery {
    saved: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct DeleteTimeOffForm {
    _csrf: Option<String>,
}

/// The user's profile zone, the one periods are typed in.
fn user_tz(user: &crate::models::User) -> Tz {
    user.timezone.parse().unwrap_or(Tz::UTC)
}

/// "December 21, 2026 to December 24, 2026", "December 21, 2026, 14:00 to
/// 17:30", "December 21, 2026, from 14:00", or one date alone for a single
/// whole day, in the zone the period was entered in. The end is exclusive,
/// so a period ending at midnight is shown as ending the day before, and
/// only times that are not a day boundary are written out.
pub(super) fn period_range_label(period: &Period, lang: &str) -> String {
    let start = period.local_start();
    let end = period.local_end();
    let (from_midnight, to_midnight) = (
        period.starts_on_day_boundary(),
        period.ends_on_day_boundary(),
    );
    // The last day the period touches: the day before an end at midnight.
    let last_day = if to_midnight {
        end.date()
            .pred_opt()
            .unwrap_or(end.date())
            .max(start.date())
    } else {
        end.date()
    };
    let date = |d: NaiveDate| crate::i18n::format_date(d, lang);
    let hm = |t: NaiveDateTime| t.format("%H:%M").to_string();
    let fill = |key: &str, pairs: &[(&'static str, String)]| {
        let mut args = fluent_bundle::FluentArgs::new();
        for (name, value) in pairs {
            args.set(*name, value.clone());
        }
        crate::i18n::translate(lang, key, Some(&args))
    };
    let at = |t: NaiveDateTime| fill("time-off-at", &[("date", date(t.date())), ("time", hm(t))]);
    if start.date() == last_day {
        let day = date(start.date());
        return match (from_midnight, to_midnight) {
            (true, true) => day,
            (false, true) => fill("time-off-from-time", &[("date", day), ("time", hm(start))]),
            (true, false) => fill("time-off-until-time", &[("date", day), ("time", hm(end))]),
            (false, false) => fill(
                "time-off-same-day",
                &[("date", day), ("start", hm(start)), ("end", hm(end))],
            ),
        };
    }
    let from = if from_midnight {
        date(start.date())
    } else {
        at(start)
    };
    let to = if to_midnight { date(last_day) } else { at(end) };
    fill("time-off-range", &[("start", from), ("end", to)])
}

async fn render_time_off(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    form: &TimeOffForm,
    error: Option<&str>,
    flash: Option<String>,
) -> Response {
    let user = &auth_user.user;
    let lang = auth_user.lang;
    let now = Utc::now();
    let periods = match time_off::upcoming(&state.pool, &user.id, now).await {
        Ok(periods) => periods,
        Err(e) => return internal_error_response("load time off", &e),
    };
    let tz = user_tz(user);
    let impersonated = auth_user.impersonation.is_some();
    let periods_ctx: Vec<minijinja::Value> = periods
        .iter()
        .map(|p| {
            context! {
                id => &p.id,
                range => period_range_label(p, lang),
                in_progress => p.in_progress(now),
                label => if impersonated { "" } else { p.label.as_deref().unwrap_or("") },
                // Shown when the profile zone has changed since.
                other_timezone => if p.tz != tz { p.timezone.as_str() } else { "" },
            }
        })
        .collect();
    let tmpl = match state.templates.get_template("time_off.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };
    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    let today = now.with_timezone(&tz).date_naive();
    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(auth_user, "time-off"),
            lang => lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
            periods => periods_ctx,
            timezone => tz.name(),
            today => today.format("%Y-%m-%d").to_string(),
            error => error.map(|key| crate::i18n::translate(lang, key, None)),
            flash => flash,
            form_start_date => &form.start_date,
            form_start_time => &form.start_time,
            form_end_date => &form.end_date,
            form_end_time => &form.end_time,
            form_label => &form.label,
        })
        .unwrap_or_default(),
    )
    .into_response()
}

pub(super) async fn time_off_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<TimeOffQuery>,
) -> Response {
    let flash = match query.saved.as_deref() {
        Some("added") => Some(crate::i18n::translate(
            auth_user.lang,
            "time-off-added",
            None,
        )),
        Some("deleted") => Some(crate::i18n::translate(
            auth_user.lang,
            "time-off-deleted",
            None,
        )),
        _ => None,
    };
    render_time_off(&state, &auth_user, &TimeOffForm::default(), None, flash).await
}

pub(super) async fn add_time_off(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<TimeOffForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    if let Some(resp) = refuse_impersonated_change(&auth_user, "add") {
        return resp;
    }
    let user = &auth_user.user;
    let tz = user_tz(user);
    let now = Utc::now();
    let parsed = time_off::normalize_label(&form.label).and_then(|label| {
        time_off::parse_period(
            &form.start_date,
            &form.start_time,
            &form.end_date,
            &form.end_time,
            tz,
            now,
        )
        .map(|(start, end)| (start, end, label))
    });
    let (start, end, label) = match parsed {
        Ok(p) => p,
        Err(e) => return render_time_off(&state, &auth_user, &form, Some(e.key()), None).await,
    };
    match time_off::add(
        &state.pool,
        &user.id,
        start,
        end,
        tz.name(),
        label.as_deref(),
    )
    .await
    {
        Ok(Some(id)) => {
            tracing::info!(user_id = %user.id, time_off_id = %id, "time off added");
            Redirect::to("/dashboard/time-off?saved=added").into_response()
        }
        Ok(None) => {
            render_time_off(
                &state,
                &auth_user,
                &form,
                Some("time-off-error-too-many"),
                None,
            )
            .await
        }
        Err(e) => internal_error_response("add time off", &e),
    }
}

pub(super) async fn delete_time_off(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<DeleteTimeOffForm>,
) -> Response {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    if let Some(resp) = refuse_impersonated_change(&auth_user, "delete") {
        return resp;
    }
    let user = &auth_user.user;
    match time_off::delete(&state.pool, &user.id, &id).await {
        Ok(true) => {
            tracing::info!(user_id = %user.id, time_off_id = %id, "time off deleted");
            Redirect::to("/dashboard/time-off?saved=deleted").into_response()
        }
        // Not theirs, or already gone: nothing to say.
        Ok(false) => Redirect::to("/dashboard/time-off").into_response(),
        Err(e) => internal_error_response("delete time off", &e),
    }
}
