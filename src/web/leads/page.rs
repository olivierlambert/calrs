//! What the public pages and the admin card need to know about lead capture.

use minijinja::context;
use sqlx::SqlitePool;

use crate::web::is_safe_company_link;

/// The `lead` value the public slot and booking templates render with.
///
/// Always carries the same keys so a template never has to guard against a
/// missing one. `active` already folds in the admin switch and the event
/// type's own toggle.
pub(in crate::web) async fn page_ctx(pool: &SqlitePool, event_type_id: &str) -> minijinja::Value {
    let global = crate::leads::config::global_settings(pool).await;
    let active = global.enabled
        && crate::leads::config::event_type_capture_enabled(pool, event_type_id).await;
    if !active {
        return context! {
            active => false,
            retention_days => global.retention_days,
            legal_mentions_url => "",
            ask_phone => false,
        };
    }
    // The gate only asks for a phone number when the event type collects one
    // for SMS; that setting, not a lead-capture one, decides.
    let ask_phone = crate::leads::config::event_type_collects_phone(pool, event_type_id).await;
    context! {
        active => true,
        retention_days => global.retention_days,
        legal_mentions_url => legal_mentions_url(pool).await.unwrap_or_default(),
        ask_phone => ask_phone,
    }
}

/// The `lead` value for pages that never capture: the ad-hoc combined pages
/// (`/u/alice+bob/...`), whose URL the capture endpoint does not resolve.
pub(in crate::web) fn page_off() -> minijinja::Value {
    context! {
        active => false,
        retention_days => crate::leads::config::GLOBAL_DEFAULT_RETENTION_DAYS,
        legal_mentions_url => "",
        ask_phone => false,
    }
}

/// The admin-configured privacy page. `None` when unset, or when the stored
/// value is anything but `http(s)://`, as a backstop against a hand-edited row.
pub(super) async fn legal_mentions_url(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT legal_mentions_url FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
    .filter(|s| !s.is_empty() && is_safe_company_link(s))
}

/// The values the admin card renders with.
pub(in crate::web) async fn admin_ctx(pool: &SqlitePool) -> minijinja::Value {
    let global = crate::leads::config::global_settings(pool).await;
    context! {
        enabled => global.enabled,
        retention_days => global.retention_days,
        legal_mentions_url => legal_mentions_url(pool).await.unwrap_or_default(),
    }
}
