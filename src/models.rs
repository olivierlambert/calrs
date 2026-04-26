use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: String,
    pub timezone: String,
    pub created_at: String,
    pub updated_at: String,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub timezone: String,
    pub password_hash: Option<String>,
    pub role: String,
    pub auth_provider: String,
    pub oidc_subject: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub username: Option<String>,
    pub booking_email: Option<String>,
    pub title: Option<String>,
    pub bio: Option<String>,
    pub avatar_path: Option<String>,
    pub allow_dynamic_group: bool,
    pub language: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AuthConfig {
    pub id: String,
    pub registration_enabled: bool,
    pub allowed_email_domains: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub oidc_enabled: bool,
    pub oidc_issuer_url: Option<String>,
    pub oidc_client_id: Option<String>,
    pub oidc_client_secret: Option<String>,
    pub oidc_auto_register: bool,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub source: String,
    pub oidc_id: Option<String>,
    pub created_at: String,
    pub slug: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct CaldavSource {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub password_enc: Option<String>,
    pub last_synced: Option<String>,
    pub sync_token: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Calendar {
    pub id: String,
    pub source_id: String,
    pub href: String,
    pub display_name: Option<String>,
    pub color: Option<String>,
    pub ctag: Option<String>,
    pub is_busy: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub calendar_id: String,
    pub uid: String,
    pub etag: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub all_day: bool,
    pub timezone: Option<String>,
    pub rrule: Option<String>,
    pub status: Option<String>,
    pub raw_ical: Option<String>,
    pub synced_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct EventType {
    pub id: String,
    pub account_id: String,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub duration_min: i32,
    pub location_type: String,
    pub location_value: Option<String>,
    pub buffer_before: i32,
    pub buffer_after: i32,
    pub min_notice_min: i32,
    pub enabled: bool,
    pub created_at: String,
    pub group_id: Option<String>,
    pub created_by_user_id: Option<String>,
    pub is_private: bool, // deprecated — use `visibility` column
    pub visibility: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct BookingInvite {
    pub id: String,
    pub event_type_id: String,
    pub token: String,
    pub guest_name: String,
    pub guest_email: String,
    pub message: Option<String>,
    pub expires_at: Option<String>,
    pub max_uses: i32,
    pub used_count: i32,
    pub created_by_user_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AvailabilityRule {
    pub id: String,
    pub event_type_id: String,
    pub day_of_week: i32,
    pub start_time: String,
    pub end_time: String,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AvailabilityOverride {
    pub id: String,
    pub event_type_id: String,
    pub date: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub is_blocked: bool,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Booking {
    pub id: String,
    pub event_type_id: String,
    pub uid: String,
    pub guest_name: String,
    pub guest_email: String,
    pub guest_timezone: String,
    pub notes: Option<String>,
    pub start_at: String,
    pub end_at: String,
    pub status: String,
    pub cancel_token: String,
    pub reschedule_token: String,
    pub created_at: String,
    pub assigned_user_id: Option<String>,
}
