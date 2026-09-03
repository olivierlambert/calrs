# calrs — Claude Code Context

## Project overview

`calrs` is an open-source scheduling platform written in Rust. It is a self-hostable alternative to Cal.com, starting as a CLI tool before adding a web interface. The project is named **calrs** (potential domain: `cal.rs`).

**Core concept:** Connect your CalDAV calendar(s), define bookable meeting types with availability rules, and eventually share a booking link. No Node.js, no PostgreSQL, no SaaS subscription.

**License:** AGPL-3.0

---

## Tech stack

| Concern | Choice | Notes |
|---|---|---|
| Language | Rust (2021 edition) | Targeting stable |
| CLI | `clap` v4 (derive API) | Subcommand tree pattern |
| Async runtime | `tokio` (full features) | Used throughout |
| Database | SQLite via `sqlx` 0.7 | WAL mode, foreign keys enabled, migrations inlined |
| HTTP client | `reqwest` (rustls, no openssl) | CalDAV PROPFIND/REPORT and EWS SOAP requests |
| Calendar providers | trait `CalendarProvider` (`src/providers/`) | Pluggable back-ends: CalDAV, EWS (Exchange 2019). Sync/source code dispatches via the trait. |
| Async traits | `async-trait` 0.1 | Object-safe `dyn CalendarProvider`. |
| XML parsing | `quick-xml` 0.31 | CalDAV responses are XML over WebDAV |
| iCal | `icalendar` crate | Parsing/generating VEVENT data |
| Time | `chrono` + `chrono-tz` | Timezone handling is a known complexity area |
| IDs | `uuid` v1 | UUID v4 for all primary keys |
| Terminal output | `colored` + `tabled` | Colored text and ASCII tables in CLI output |
| Web server | `axum` 0.8 | HTTP booking page, served from CLI |
| Templates | `minijinja` 2 | Jinja2-compatible, loaded from `templates/` dir |
| Encryption | `aes-gcm` | AES-256-GCM encryption for stored credentials |
| Auth | `argon2` + `password-hash` | Argon2 password hashing for local accounts |
| Auth (OIDC) | `openidconnect` 4.x | OpenID Connect SSO (Keycloak, etc.) with PKCE |
| Sessions | `axum-extra` (cookies) | Server-side sessions in SQLite, HttpOnly cookies |
| Email | `lettre` 0.11 | SMTP with STARTTLS, async tokio transport |
| Logging | `tracing` + `tracing-subscriber` | Structured logging with env-filter |
| HTTP tracing | `tower-http` 0.6 | TraceLayer for request-level observability |
| Error handling | `anyhow` (app-level) + `thiserror` (lib-level) | Standard Rust pattern |
| Config/paths | `directories` crate | XDG-compliant data dir: `$XDG_DATA_HOME/calrs` |

---

## Project structure

```
calrs/
├── Cargo.toml
├── CLAUDE.md                     ← you are here
├── README.md
├── .gitignore
├── assets/
│   └── intl-tel-input/           ← vendored phone widget (MIT) + libphonenumber utils.js (Apache-2.0)
├── migrations/
│   ├── 001_initial.sql           ← full SQLite schema
│   ├── 002_auth.sql              ← users, sessions, auth_config, groups
│   ├── 003_username.sql          ← username column on users
│   ├── 004_oidc.sql              ← OIDC columns on auth_config
│   ├── 005_requires_confirmation.sql ← requires_confirmation on event_types
│   ├── 006_group_event_types.sql ← slug on groups, group_id on event_types, assigned_user_id on bookings
│   ├── 007_caldav_write.sql      ← write_calendar_href on caldav_sources, caldav_calendar_href on bookings
│   ├── 008_recurrence_id.sql     ← recurrence_id column on events
│   ├── 009_uid_recurrence_unique.sql ← composite unique index (uid, recurrence_id) on events
│   ├── 010_confirm_token.sql     ← confirm_token on bookings for email approve/decline
│   ├── 011_event_type_calendars.sql ← junction table for per-event-type calendar selection
│   ├── 012_reminders.sql         ← reminder_minutes on event_types, reminder_sent_at on bookings
│   ├── 013_booking_email.sql     ← booking_email on users
│   ├── 014_team_links.sql        ← team_links, team_link_members, team_link_bookings tables
│   ├── 015_user_profile.sql      ← title, bio, avatar_path on users
│   ├── 016_booking_unique.sql    ← partial unique index for double-booking prevention
│   ├── 017_events_per_calendar.sql ← per-calendar event uniqueness (uid, calendar_id)
│   ├── 018_private_invites.sql   ← is_private on event_types, booking_invites table
│   ├── 019_team_link_reusable.sql ← one_time_use column on team_links
│   ├── 020_booking_attendees.sql ← max_additional_guests on event_types, booking_attendees table
│   ├── 021_accent_color.sql      ← accent_color on auth_config
│   ├── 022_theme.sql             ← theme preset + custom color columns on auth_config
│   ├── 023_team_link_windows.sql ← availability_windows on team_links
│   ├── 024_team_link_features.sql ← location, description, reminder on team_links
│   ├── 025_reschedule_by_host.sql ← reschedule_by_host flag on bookings
│   ├── 026_visibility.sql        ← visibility column on event_types (public/internal/private)
│   ├── 027_calendar_sync_token.sql ← sync_token on calendars
│   ├── 028_company_link.sql      ← company_link URL on auth_config
│   ├── 029_scheduling_mode.sql   ← scheduling_mode on event_types (round_robin/collective)
│   ├── 030_member_weight.sql     ← weight on user_groups for round-robin priority
│   ├── 031_fix_legacy_timezones.sql ← fix bare timezone names to IANA identifiers
│   ├── 032_event_type_member_weights.sql ← per-event-type member weights table
│   ├── 033_group_profile.sql     ← description and avatar_path on groups
│   ├── 034_teams.sql             ← unified teams: teams, team_members, team_groups tables; migrates groups + team_links
│   ├── 035_drop_legacy_team_links.sql ← drops legacy team_links tables
│   ├── 036_default_calendar_view.sql ← default_calendar_view on event_types
│   ├── 037_booking_frequency_limits.sql ← booking_frequency_limits table
│   ├── 038_first_slot_only.sql   ← first_slot_only on event_types
│   ├── 039_allow_dynamic_group.sql ← allow_dynamic_group opt-out on users
│   ├── 040_user_availability.sql ← per-user default working hours (user_availability_rules)
│   ├── 041_last_full_sync.sql    ← last_full_sync timestamp on caldav_sources
│   ├── 042_event_transp.sql      ← TRANSP column on events (skip TRANSPARENT)
│   ├── 043_event_type_watchers.sql ← event_type_watchers junction (team watches event type)
│   ├── 044_booking_claim.sql     ← claimed_by_user_id/claimed_at on bookings + booking_claim_tokens
│   ├── 055_provider_type.sql     ← provider_type on caldav_sources (caldav/ews) for the calendar-provider abstraction
│   ├── 056_meeting_links.sql     ← jitsi + webhook meeting-provider columns on auth_config
│   ├── 057_runtime_settings.sql  ← base_url + allow_private_hosts on auth_config (env-overridable runtime settings)
│   ├── 058_resources.sql         ← shared resources: resources, resource_events, event_type_resources; resource_scheduling_mode, assigned_resource_id, lend_resource_write
│   ├── 059_resource_sync_error.sql ← last_sync_error on resources (feed failure indicator)
│   ├── 062_sms_notifications.sql ← SMS: guest_phone on bookings, sms_phone_mode on event_types, sms_config + sms_usage tables, sms_allow_all_users on auth_config
│   └── 063_booking_horizon.sql   ← booking_horizon_days on event_types (NULL = unlimited)
├── templates/
│   ├── base.html                 ← base layout + CSS (light/dark mode)
│   ├── dashboard_base.html       ← sidebar layout (extends base.html, all dashboard pages extend this)
│   ├── auth/
│   │   ├── login.html            ← login page (local + SSO button)
│   │   └── register.html         ← registration page
│   ├── dashboard_overview.html   ← overview with stats (extends dashboard_base)
│   ├── dashboard_event_types.html ← event types listing (extends dashboard_base)
│   ├── dashboard_bookings.html   ← bookings listing (extends dashboard_base)
│   ├── dashboard_sources.html    ← calendar sources (extends dashboard_base)
│   ├── dashboard_teams.html      ← teams listing (extends dashboard_base)
│   ├── dashboard_internal.html   ← internal/organization event types (extends dashboard_base)
│   ├── settings.html             ← profile & settings with avatar/title/bio (extends dashboard_base)
│   ├── admin.html                ← admin dashboard (extends dashboard_base)
│   ├── event_type_form.html      ← create/edit event types (extends dashboard_base)
│   ├── invite_form.html          ← invite management for internal/private event types (extends dashboard_base)
│   ├── source_form.html          ← add CalDAV source (extends dashboard_base)
│   ├── source_test.html          ← connection test / sync results (extends dashboard_base)
│   ├── source_write_setup.html   ← write-back calendar selection (extends dashboard_base)
│   ├── team_form.html            ← create/edit team (extends dashboard_base)
│   ├── team_settings.html        ← team settings: members, linked groups, danger zone (extends dashboard_base)
│   ├── troubleshoot.html         ← availability troubleshoot timeline (extends dashboard_base)
│   ├── overrides.html            ← date overrides management per event type (extends dashboard_base)
│   ├── profile.html              ← public user profile (with avatar, title, bio)
│   ├── team_profile.html         ← public team page
│   ├── slots.html                ← available time slots (with timezone picker)
│   ├── book.html                 ← booking form
│   ├── confirmed.html            ← confirmation / pending page
│   ├── booking_approved.html     ← token-based approve success page
│   ├── booking_decline_form.html ← token-based decline form (optional reason)
│   ├── booking_declined.html     ← token-based decline success page
│   ├── booking_cancel_form.html  ← guest self-cancel form (optional reason)
│   ├── booking_cancelled_guest.html ← guest self-cancel success page
│   ├── booking_host_reschedule.html ← host-initiated reschedule page
│   ├── booking_reschedule_confirm.html ← reschedule confirmation page
│   ├── booking_action_error.html ← error page for invalid/expired tokens
│   ├── booking_claim_form.html   ← watcher claim form (token-based)
│   ├── booking_claimed.html      ← claim success page
│   └── booking_already_claimed.html ← claim collision page (another watcher got there first)
└── src/
    ├── main.rs                   ← CLI entry point, Cli/Commands enum, tokio main
    ├── db.rs                     ← SQLite pool setup (WAL mode) + migration runner
    ├── models.rs                 ← domain structs: Account, User, Session, AuthConfig,
    │                               CaldavSource, Calendar, Event, EventType, Booking
    ├── crypto.rs                 ← AES-256-GCM encryption for stored credentials,
    │                               secret key management, legacy password migration
    ├── auth.rs                   ← authentication: password hashing, sessions, OIDC,
    │                               axum extractors (AuthUser, AdminUser), web handlers
    ├── email.rs                  ← SMTP email with .ics calendar invites, HTML templates
    ├── resources.rs              ← shared bookable resources: feed sync, busy intervals,
    │                               mode merge (all/round_robin), booking-time check
    ├── rrule.rs                  ← RRULE expansion (DAILY/WEEKLY/MONTHLY, EXDATE, BYDAY)
    ├── settings.rs               ← runtime settings (base_url, allow_private_hosts): env-overrides-DB, process-global cache
    ├── sms/                      ← SMS notifications, provider-agnostic
    │   ├── mod.rs                ← SmsProvider trait, SmsError/SendReceipt, config load (env + DB), notify_guest()
    │   ├── factory.rs            ← provider kinds, ProviderSpec registry (drives the admin form), validate_config()
    │   ├── phone.rs              ← E.164 normalisation + country calling codes
    │   ├── message.rs            ← localised bodies (Fluent) + GSM-7 segment estimation
    │   ├── twilio.rs             ← Twilio adapter
    │   ├── gatewayapi.rs         ← GatewayApi adapter (.com / .eu regions)
    │   ├── sevenio.rs            ← seven.io adapter (HTTP 200 + in-body error codes)
    │   └── webhook.rs            ← generic webhook adapter (bring your own gateway, HMAC-signed)
    ├── utils.rs                  ← shared utilities: split_vevents(), extract_vevent_field()
    ├── caldav/
    │   └── mod.rs                ← CalDAV client: discovery, calendar list, event fetch, write-back
    ├── web/
    │   └── mod.rs                ← Axum web server: dashboard, booking, admin panel, token actions
    └── commands/
        ├── mod.rs                ← re-exports all subcommands
        ├── source.rs             ← `calrs source add/list/remove/test`
        ├── sync.rs               ← `calrs sync [--full]` — pull CalDAV → SQLite
        ├── calendar.rs           ← `calrs calendar show`
        ├── event_type.rs         ← `calrs event-type create/list/slots`
        ├── booking.rs            ← `calrs booking create/list/cancel`
        ├── config.rs             ← `calrs config smtp/show/smtp-test/auth/oidc`
        ├── resource.rs           ← `calrs resource probe`, feed/CalDAV probing
        └── user.rs               ← `calrs user create/list/promote/set-password`
```

---

## Database schema (SQLite)

Migrations are tracked via `_migrations` table and run incrementally at startup via `db::migrate()`.

Key tables:

- **`users`** — multi-user: email, name, password_hash (argon2), role (admin/user), auth_provider (local/oidc), oidc_subject, username (unique), enabled flag, title, bio, avatar_path
- **`sessions`** — server-side sessions: token (PK), user_id, expires_at (30-day TTL)
- **`auth_config`** — singleton: registration_enabled, allowed_email_domains, OIDC settings (issuer, client_id, client_secret, auto_register)
- **`accounts`** — scheduling accounts linked to users via `user_id`
- **`caldav_sources`** — CalDAV server connections (URL, credentials, sync state, `write_calendar_href`). `enabled` flag, `ON DELETE CASCADE`
- **`calendars`** — calendar collections discovered under a source; `is_busy=1` means events block availability
- **`events`** — cached remote events from CalDAV sync; unique on `(uid, calendar_id, COALESCE(recurrence_id, ''))`, stores `raw_ical`, `etag`, `rrule`, `all_day`, `timezone`, `recurrence_id`, `status`
- **`event_types`** — bookable meeting templates (slug unique per account, `duration_min`, `buffer_before`/`buffer_after`, `min_notice_min`, `location_type`/`location_value`, `requires_confirmation`, `visibility` (public/internal/private), `max_additional_guests`, `group_id` (legacy), `team_id` (unified teams FK), `created_by_user_id`, `reminder_minutes`, `scheduling_mode` (round_robin/collective), `default_calendar_view` (month/week/column), `first_slot_only` (boolean))
- **`availability_rules`** — weekly recurring windows per event type (day_of_week 0=Sun…6=Sat, HH:MM times)
- **`availability_overrides`** — date-specific exceptions (day off, special hours). `is_blocked` flag
- **`bookings`** — bookings with `uid` (iCal), guest info, status (confirmed/pending/cancelled/declined), `cancel_token`/`reschedule_token`/`confirm_token`, `assigned_user_id` (for group round-robin), `caldav_calendar_href` (write-back tracking), `reminder_sent_at` (tracks when reminder email was sent)
- **`smtp_config`** — SMTP server settings (host, port, credentials, sender), one per account
- **`event_type_calendars`** — junction table linking event types to specific calendars for per-event-type calendar selection. Empty = use all `is_busy=1` calendars (backward-compatible default)
- **`booking_invites`** — tokenized invite links for internal/private event types: `token` (unique), `event_type_id`, `guest_name`, `guest_email`, `message`, `expires_at`, `max_uses`, `used_count`, `created_by_user_id`
- **`booking_attendees`** — additional attendees per booking: `booking_id` (FK), `email`, `created_at`
- **`teams`** — unified teams replacing both OIDC groups-as-scheduling-units and ad-hoc team links. Fields: `name`, `slug` (unique), `description`, `avatar_path`, `visibility` (public/private), `invite_token` (for private teams), `created_by`
- **`team_members`** — team membership: `team_id`, `user_id`, `role` (admin/member), `source` (direct/group). Source tracks whether membership comes from direct assignment or OIDC group sync
- **`team_groups`** — links teams to OIDC groups for automatic member sync: `team_id`, `group_id`
- **`event_type_member_weights`** — per-event-type round-robin priority: `event_type_id`, `user_id`, `weight` (higher = assigned first)
- **`booking_frequency_limits`** — per-event-type booking caps: `event_type_id`, `max_bookings`, `period` (day/week/month/year)
- **`groups`** / **`user_groups`** — preserved for OIDC identity sync from Keycloak. Groups are no longer used directly for scheduling; teams reference groups via `team_groups` for automatic member sync. `user_groups.weight` for round-robin priority
- **`team_links`** — legacy table, migrated to private teams by migration 034. No longer used by the application

All primary keys are UUID v4 strings. Datetimes are ISO8601 strings.

---

## CalDAV client

File: `src/caldav/mod.rs`

The client is intentionally minimal — enough to be useful, not a full RFC 4791 implementation.

**Discovery flow** (three-step, RFC 4791 compliant):
1. `discover_principal()` — PROPFIND Depth:0 on base URL, extracts `<d:current-user-principal>` href
2. `discover_calendar_home(principal)` — PROPFIND Depth:0 on principal, extracts `<cal:calendar-home-set>` href
3. `list_calendars(home_url)` — PROPFIND Depth:1 on calendar home, filters to `<cal:calendar/>` resource types only

**Other methods:**
- `check_connection()` — OPTIONS request, verifies `calendar-access` in DAV header
- `fetch_events(calendar_href)` — REPORT with `calendar-query` filter for VEVENTs (60s timeout)
- `fetch_events_since(calendar_href, since_utc)` — REPORT with RFC 4791 `time-range` filter (only future events). Falls back to full fetch if the server rejects the time-range query.
- `put_event(calendar_href, uid, ics)` — PUT a VEVENT to the calendar (write-back)
- `delete_event(calendar_href, uid)` — DELETE a VEVENT from the calendar

**URL resolution:** All hrefs from the server are resolved via `resolve_url()` which uses the server origin (scheme + host), not the base URL path, to avoid path duplication.

**XML templates** are `const &str` at the bottom of the file (PROPFIND_PRINCIPAL, PROPFIND_CALENDAR_HOME, PROPFIND_CALENDARS, REPORT_CALENDAR_DATA).

**Timeouts:** 10s default for discovery/metadata requests, 60s for event fetches (calendars can have thousands of events).

**Tested with:** BlueMind (4000+ events). Handles both `aic:` and `x1:` namespace prefixes for calendar colors, `cso:` and `cs:` for ctags.

**Known limitation:** The XML parser is a simple string-based tag extractor. It works for well-formed CalDAV responses but is not robust against malformed or deeply nested XML. A future improvement would be to use `quick-xml` + serde derive.

**iCal parsing:** `split_vevents()` and `extract_vevent_field()` in `utils.rs` split multi-VEVENT CalDAV blobs (e.g. BlueMind recurring events with modified instances) into individual VEVENT blocks and extract fields. Used by both CLI sync and web sync. Dates are stored as-is from iCal: `YYYYMMDD` for all-day events, `YYYYMMDDTHHMMSS` for timed events.

**Multi-VEVENT sync:** CalDAV resources may contain multiple VEVENTs (parent with RRULE + modified instances with RECURRENCE-ID). The sync splits them and stores each as a separate row with a composite unique key `(uid, COALESCE(recurrence_id, ''))`.

---

## Authentication & authorization

File: `src/auth.rs`

**Local auth:** Argon2 password hashing. Server-side sessions stored in SQLite with 30-day TTL. HttpOnly cookies (`calrs_session`).

**OIDC:** OpenID Connect via `openidconnect` 4.x crate. Authorization code flow with PKCE (S256). State, nonce, and PKCE verifier stored in short-lived cookies during the flow. Tested with Keycloak.

**User linking:** On OIDC callback, tries: (1) match by `oidc_subject`, (2) match by email (links existing local user), (3) auto-register if enabled. On login, `groups` and `title` JWT claims are extracted via `extract_claims_from_id_token()` and synced to the user record.

**Extractors:** `AuthUser` (redirects to login if not authenticated), `AdminUser` (returns 403 if not admin). Both implemented as axum `FromRequestParts`.

**Login/register redirect:** If the user is already authenticated, visiting `/auth/login` or `/auth/register` redirects to `/dashboard` instead of showing the form.

**URL scheme:** User-scoped public booking URLs: `/u/{username}/{slug}`. Legacy single-user routes (`/{slug}`) kept for backward compatibility.

---

## Web UI

File: `src/web/mod.rs`, templates in `templates/`

**Sidebar layout** (`dashboard_base.html`): All authenticated pages use a two-column layout with a persistent left sidebar (260px). Sidebar shows user avatar (with initials fallback), name, title, and organized nav sections. Mobile responsive with hamburger menu. All dashboard sub-pages pass `sidebar => sidebar_context(&auth_user, "active-page")` to their template context.

**Dashboard** — split into focused pages, each extending `dashboard_base.html`:
- `/dashboard` — Overview with stat tiles and pending bookings
- `/dashboard/event-types` — Personal + team event types (create/edit/toggle/delete/view)
- `/dashboard/bookings` — Pending approval + upcoming bookings (cancel with optional reason)
- `/dashboard/sources` — Calendar sources (add/test/sync/remove/write-back)
- `/dashboard/teams` — Teams listing (create/edit/manage members/delete)
- `/dashboard/invite-links` — Internal event types (personal + team) visible to authenticated users, with quick invite link generation (renamed from `/dashboard/organization`)

**Admin panel** (`/dashboard/admin`): User management (promote/demote, enable/disable), auth settings (registration toggle, allowed domains), OIDC config, SMTP status, groups overview, impersonation. Requires `AdminUser`.

**Public pages:** User profile (`/u/{username}`), team profile (`/team/{slug}`), time slot picker (Cal.com-style 3-panel layout with switchable month/week/column views), booking form (with optional additional attendees), confirmation page. Event types support location (video link, phone, in-person, custom). Dark/light theme toggle on all public pages. Legacy `/g/{group-slug}` URLs redirect to `/team/{slug}`.

**Theme toggle:** Class-based dark mode (`html.dark`) with inline `<head>` script for flash-free loading from `localStorage`. Public pages have a sun/moon toggle in the footer. Dashboard users can set System/Light/Dark in Profile & Settings.

**Availability troubleshoot** (`/dashboard/troubleshoot/{event_type_id}`): Visual timeline showing why slots are available or blocked, with event details. Helps debug availability issues.

**Per-event-type calendar selection:** Event type form includes calendar checkboxes (from `is_busy=1` calendars). Selected calendars are stored in `event_type_calendars` junction table. When computing busy times, if no calendars are selected all `is_busy=1` calendars are checked (backward-compatible). The filter uses `NOT EXISTS / IN` subquery on `event_type_calendars` and is applied in `fetch_busy_times_for_user()`, troubleshoot handler, and CLI commands.

**Availability overrides:** Per-event-type date overrides at `/dashboard/event-types/{slug}/overrides`. Two types: blocked days (entire day off) and custom hours (replace weekly rules with specific time windows). Overrides are checked in `compute_slots_from_rules()` — blocked overrides skip the day, custom hours replace weekly rules. Also wired into CLI slot computation and troubleshoot view. Stored in `availability_overrides` table.

**Team event types:** Created under a team from the dashboard. Two scheduling modes: round-robin (picks the least-busy available member, with configurable per-member weights) and collective (requires ALL members to be free, with per-event-type member exclusions supported). Public URLs: `/team/{slug}/{event-slug}`. Teams can be public (listed on team profile page) or private (accessible only to members). Team admins can manage members, link OIDC groups, and configure team settings at `/dashboard/teams/{id}/settings`.

**Booking watchers:** Teams can be designated as watchers on a team event type via `event_type_watchers`. On a new booking, watchers are emailed with a tokenized "Claim this booking" link. Tokens live in `booking_claim_tokens` with an expiry. The first watcher to claim wins; subsequent claims land on `booking_already_claimed.html`. Claimed bookings surface on the watcher's dashboard via `bookings.claimed_by_user_id`.

**Event type visibility:** Three levels controlled by `visibility` column (TEXT: 'public'/'internal'/'private', migration 026). Public event types are listed on profile/group pages. Internal and private are hidden — both use tokenized invite links via `booking_invites`. Internal is available for both personal and team event types. The difference: internal event types allow **any authenticated user** to generate invite links (via the Invite Links page at `/dashboard/invite-links`), while private event types restrict invite creation to the owner. Quick link generation at `POST /dashboard/invites/{id}/quick-link` creates a single-use invite (expires 7 days) and returns JSON with the URL — available both on the Invite Links page and the per-event-type invite management page. The invite token is propagated through the booking flow via query params (`?invite=TOKEN`) and hidden form fields. Guest name/email are pre-filled from the invite (empty for quick links — guest fills them in). Token validation checks expiration and usage limits at every step. Invite management at `/dashboard/invites/{event_type_id}` includes a "Get link" button at the top for one-click link generation, plus an email form below for sending personalized invites. Invite emails use indigo accent (#6366f1).

**On-demand sync:** Slot pages (`/u/`, `/g/`, legacy `/{slug}`) and the troubleshoot view automatically sync the host's CalDAV sources if stale (>5 minutes since last sync). Uses `sync_if_stale()` from `commands/sync.rs` which calls `fetch_events_since()` with a time-range filter (RFC 4791) to only pull future events, with fallback to full fetch for servers that don't support it.

**Timezone support:** Guest timezone picker on slot pages. Browser timezone auto-detected via `Intl.DateTimeFormat`. Times displayed and booked in the guest's selected timezone.

**Avatar support:** Upload via `POST /dashboard/settings/avatar` (multipart, max 2MB, image/*). Served at `GET /avatar/{user_id}` with content-type detection. Stored in `{data_dir}/avatars/{user_id}.{ext}`. Delete via `POST /dashboard/settings/avatar/delete`.

**Admin impersonation:** Admins can impersonate any user from the admin panel to troubleshoot their view. Uses a separate `calrs_impersonate` cookie.

**Email approve/decline:** Pending bookings generate a `confirm_token`. Host notification emails include Approve/Decline buttons linking to `/booking/approve/{token}` and `/booking/decline/{token}`. These are unauthenticated public endpoints. Requires `CALRS_BASE_URL` env var.

**Guest self-cancellation:** Confirmation and pending emails include a "Cancel booking" button linking to `/booking/cancel/{cancel_token}`. Guests can cancel their own bookings with an optional reason. Cancellation updates the booking status, deletes the CalDAV event, and notifies both guest and host. Emails correctly attribute who cancelled (host vs guest).

**Booking reminders:** Background task in `calrs serve` runs every 60 seconds, sends reminder emails to both guest and host before upcoming meetings. Configurable per event type via `reminder_minutes` (NULL = no reminder). Guest reminders include a cancel button. `reminder_sent_at` on bookings prevents duplicate sends. Blue accent color (#3b82f6) for reminder emails.

**Email notifications:** Booking confirmation, cancellation, pending notice, approval request (with action buttons), decline notice — all HTML emails with plain text fallback. Confirmation and cancellation include `.ics` calendar invite attachments. Location included in emails and ICS.

**CalDAV write-back:** Confirmed bookings are pushed to the host's CalDAV calendar (if `write_calendar_href` is configured on the source). On cancellation, the event is deleted from CalDAV.

**Security hardening (1.0):**
- **CSRF protection** — double-submit cookie pattern on all 31 POST handlers via `csrf_cookie_middleware`. Client-side JS injects `_csrf` hidden field. Multipart forms use query parameter.
- **Booking rate limiting** — per-IP (10 req / 5 min) on all 4 booking handlers. Client IP is the *rightmost* `X-Forwarded-For` value (the trusted proxy's view of its peer; leftmost is attacker-controlled). See `client_ip_for_rate_limit()` in `web/mod.rs`.
- **Input validation** — server-side on all booking forms (name 1–255, email format, notes max 5000, date max 365 days), registration, settings, avatar upload (content-type whitelist).
- **Double-booking prevention** — partial unique index `idx_bookings_no_overlap` on `(event_type_id, start_at)` + `BEGIN IMMEDIATE` transactions.
- **Crash-proof handlers** — all `.unwrap()` in web handlers replaced with proper error responses.

**Observability (1.0):**
- **Structured logging** — `tracing` crate with 50 log points across auth, bookings, CalDAV, admin, email, DB migrations. Configurable via `RUST_LOG` env var (default: `calrs=info,tower_http=info`).
- **HTTP request tracing** — `tower-http` `TraceLayer` logs every request (method, path, status, latency).
- **Graceful shutdown** — SIGINT/SIGTERM handling with `with_graceful_shutdown()`, drains in-flight requests.

---

## CLI UX conventions

- Use `colored` for status: `"✓".green()`, `"✗".red()`, `"…".dimmed()`
- Use `tabled` for listing resources (sources, event types, bookings)
- Interactive prompts via `prompt()` / `prompt_with_default()` helpers
- All commands take `&SqlitePool` as first argument; commands that handle credentials also take `&[u8; 32]` secret key

---

## Captcha (trycap / Cap)

### What it does

Protects booking endpoints against bots with a privacy-first proof-of-work CAPTCHA. The feature is **opt-in**: when no configuration is stored the booking flow works exactly as before — no widget, no server-side check. The captcha is only active on booking form pages (guest-facing), never on registration or dashboard routes.

The chosen provider is [Cap](https://trycap.dev) — self-hosted, no GAFAM, no tracking, Docker-deployable. It is API-compatible with the reCAPTCHA/hCaptcha verification protocol, so switching providers in the future only requires updating `src/web/captcha.rs`.

### Configuration (admin panel)

Configured at `/dashboard/admin` → **Captcha** section. Four fields:

| Field | Required | Description |
|---|---|---|
| Instance URL | Yes | Base URL of your Cap server, e.g. `https://captcha.example.com` |
| Site key | Yes | Site key from the Cap dashboard |
| Secret | Yes | Secret key for server-to-server verification (encrypted at rest with AES-256-GCM, same pattern as OIDC client secret) |
| Widget script URL | No | Override the JS bundle URL. Defaults to `https://cdn.jsdelivr.net/npm/cap-widget`. Useful for air-gapped deployments. Changes take effect immediately after saving (CSP is rebuilt in memory). |

Leaving all three main fields empty disables the captcha. The secret uses the keep-current pattern: leaving the password field empty on save preserves the stored value.

### How it works end-to-end

**GET (booking form):** The handler reads `state.captcha_config` (a `RwLock<Option<CaptchaConfig>>`). If `Some`, it passes `captcha_enabled = true`, `captcha_api_endpoint`, and `captcha_widget_url` to the template. The `<cap-widget>` element is rendered conditionally in `templates/book.html` with all i18n attributes pre-filled via the Fluent `t()` helper.

**POST (booking submit):** The `BookForm` struct has a `captcha_token: Option<String>` field with `#[serde(rename = "cap-token")]` — the field name the Cap widget submits. After the CSRF check, `captcha::verify(&captcha_cfg, form.captcha_token.as_deref()).await` is called:
- `config = None` → `Ok(())` immediately (pass-through)
- `config = Some`, token missing/empty → `Err(())` → renders `booking_action_error.html`
- `config = Some`, token present → POST to `<instance>/<site-key>/siteverify` with JSON `{"secret": "...", "response": "<token>"}` → parses `{"success": bool}`

The verification is implemented in `src/web/captcha.rs` and applies to **3 handlers**: `handle_booking`, `handle_booking_for_user`, `handle_group_booking`. The dynamic group booking path (`username.contains('+')`) is also protected because the check happens before the branch.

### Code structure

| File | Role |
|---|---|
| `src/web/captcha.rs` | Self-contained module: `CaptchaConfig` struct, `load_captcha_config()`, `verify()`, `extract_origin()` helper, unit tests |
| `src/web/mod.rs` | `AppState.captcha_config: RwLock<Option<CaptchaConfig>>`, `build_csp()`, `csp_middleware()`, `admin_update_captcha()` handler, wiring in the 4 GET form handlers and 3 POST booking handlers |
| `migrations/053_captcha.sql` | Adds `captcha_instance_url`, `captcha_site_key`, `captcha_secret`, `captcha_widget_url` columns to `auth_config` |
| `templates/admin.html` | Captcha configuration section (status badge, 4 inputs, save button) |
| `templates/book.html` | Conditional `<cap-widget>` with all `data-cap-i18n-*` attributes |
| `templates/base.html` | CSS custom property overrides for `cap-widget` theming |

### Content-Security-Policy

The CSP is **dynamic** — it is rebuilt whenever the admin saves captcha settings, without a server restart (except for the widget script URL, which controls the `script-src` domain). It is stored as a pre-built string in `AppState.csp: RwLock<String>` and applied by `csp_middleware` (an Axum `from_fn_with_state` layer).

`build_csp(captcha: &Option<CaptchaConfig>) -> String` produces:

**Without captcha configured:**
```
default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline';
script-src 'self' 'unsafe-inline';
connect-src 'self';
object-src 'none'; base-uri 'self'; frame-ancestors 'self'
```

**With captcha configured:**
```
default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline';
script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' <widget-origin>;
worker-src blob:;
connect-src 'self' <instance-origin> <widget-origin>;
object-src 'none'; base-uri 'self'; frame-ancestors 'self'
```

The three extra directives and why they are needed:
- **`'wasm-unsafe-eval'` in `script-src`** — Cap's proof-of-work solver runs as a WASM module. Without this, browsers block `WebAssembly.instantiate()` under a strict CSP. Falls back to a JS solver if blocked, but WASM is much faster.
- **`worker-src blob:`** — The Cap widget spawns a Web Worker via a `blob:` URL to run the solver off the main thread. `worker-src` is not inherited from `script-src` in all browsers, so it must be explicit.
- **`<widget-origin>` in `connect-src`** — The widget's JS fetches the WASM binary from the same CDN origin via `fetch()`. This is controlled by `connect-src`, not `script-src`. Both the Cap server origin (for token verification XHR) and the widget CDN origin (for WASM fetch) must be in `connect-src`.

The `csp_middleware` skips setting the header if it is already present, so individual handlers can override it if needed in the future.

### Translations

All visible strings in the Cap widget are localised via the standard Fluent system. Keys use the `captcha-` prefix and live in each language file:

Keys: `captcha-label`, `captcha-initial-state`, `captcha-verifying`, `captcha-solved`, `captcha-error`, `captcha-troubleshooting`, `captcha-wasm-disabled`, plus five `*-aria` keys for accessibility.

### Styling

The `<cap-widget>` custom element exposes CSS custom properties. They are overridden globally in `templates/base.html` to follow the app's existing theme variables:

```css
cap-widget {
  --cap-background: var(--surface);
  --cap-border-color: var(--border);
  --cap-color: var(--text);
  --cap-checkbox-background: var(--surface-hover);
  --cap-spinner-color: var(--text);
  --cap-spinner-background-color: var(--border);
}
```

Because `--surface`, `--border`, etc. are already overridden by `html.dark { ... }` (in `base.html`) and by each preset/custom theme (via `theme_css` in `AppState`), the widget automatically adapts to dark mode and all custom themes (Nord, Dracula, Tokyo Night, etc.) with no additional CSS.

---

## Shared resources

**Concept:** Instance-level bookable resources (demo lab, meeting room) backed by a read-only ICS publish feed (BlueMind "calendar address", Nextcloud public link). Resources are attached to event types by admins; a busy resource blocks booking slots. The feature is opt-in: with no resources configured, nothing changes anywhere (one no-op query per slot computation).

**Tables** (migration 058): `resources` (name, `feed_url`, optional `caldav_url` + encrypted service-account credentials, `last_synced_at`), `resource_events` (cached feed VEVENTs, unique on `(resource_id, uid, COALESCE(recurrence_id, ''))`), `event_type_resources` (junction). Columns: `event_types.resource_scheduling_mode` ('all' default / 'round_robin'), `bookings.assigned_resource_id`, `users.lend_resource_write`.

**Modes:** `all` = every attached resource must be free (busy union blocks). `round_robin` = one free resource is enough (busy intersection blocks); at booking time the least-loaded free resource is picked and stored on `bookings.assigned_resource_id`.

**Read path** (`src/resources.rs`): feeds are cached in `resource_events` and re-synced when older than 5 minutes (`sync_if_stale`; the feed is authoritative, orphans are deleted; failed fetches also stamp `last_synced_at` so dead feeds back off, and record `last_sync_error` for the admin panel; success clears it). `fetch_feed` re-validates the URL against the private-host policy on every fetch, follows no redirects, and caps body size and event count. `busy_for_resource()` merges feed events (single + RRULE-expanded, skipping CANCELLED/TRANSPARENT, excluding the booking's own reservation and reservations of cancelled bookings) with calrs' own confirmed bookings, so two event types sharing a resource cannot double-book it even without write-back. `blocking_intervals_for_event_type()` feeds `compute_slots()`; `check_and_pick()` is the booking-time gate, serialized by a process-wide lock (`booking_lock()`) held from the check until the booking row is committed. Both approval paths re-check resources (pending bookings do not block them). The troubleshoot view shows blocked intervals as `resource_busy`.

**Write path** (reservation, `src/web/mod.rs`): confirmed bookings are PUT into the required resources' CalDAV collections under the booking's own uid (`resource_push_booking`; every attached resource in `all` mode, only the assigned resource in `round_robin`), and deleted when a confirmed booking is cancelled or rescheduled (`resource_delete_booking`, targets derived from the stored assignment so detached resources still get released). Declined pending bookings need no cleanup: reservations are only pushed on confirmation. The CalDAV URL is either configured or derived from a BlueMind publish URL (`derive_caldav_url`: `/api/calendars/publish/calendar:UID/...` maps to `/dav/calendars/__uids__/UID/calendar:UID/`). Credential candidates in trust order (`resource_write_candidates`): the resource's service account first, then members who opted in via `users.lend_resource_write` and have a CalDAV source on the same scheme+host+port, preferring the booking's assigned host. Note: lending grants writes to any collection on that origin the member's own server-side ACL allows; the service account is always preferred. Write failure is non-fatal: the DB-side busy check keeps blocking the resource, and failures are logged.

**Visibility:** host-facing emails (new booking, approval request, confirmed, host reminder) show a "Resource" row via `BookingDetails.resource_name` (`booking_resource_label()`: assigned resource in round_robin, attached names in 'all'); guests never see resource names. The event-types listing shows a "resources" badge, and the bookings dashboard shows the assigned resource per booking.

**Admin UI:** `/dashboard/admin` Resources card: add (feed validated and synced on create, name auto-filled from `X-WR-CALNAME`), edit (keep-current password pattern; feed re-validated and re-synced), delete, "Sync now", "Test write" (PUT/verify/DELETE cycle with a temp event 24h out). Members opt in to credential lending in Profile & Settings. The event type form gains a "Required resources" checkbox section + mode radio, visible to admins only.

**CLI:** `calrs resource probe --url <URL> [--username U] [--write-test]` probes a feed or CalDAV collection (full RFC 4791 discovery fallback, write test with a temporary event). Known gap: `calrs event-type slots` and `calrs booking create` do not consult resources yet; the web paths do.

**Files:** `src/resources.rs` (core logic + tests), `src/web/mod.rs` (admin handlers, booking wiring, write-back), `src/commands/resource.rs` (probe CLI), `migrations/058_resources.sql`, `templates/admin.html`, `templates/event_type_form.html`, `templates/settings.html`, `templates/troubleshoot.html`.

---

## SMS notifications

**Concept:** optional text-message notifications to the guest, opt-in per event type. Off by default twice over: with no `sms_config` row *and* every event type leaving `sms_phone_mode` at `off`, nothing in `src/sms/` ever runs and the booking flow is byte-for-byte what it was.

**Provider abstraction:** `SmsProvider` (in `src/sms/mod.rs`) is the same shape as `CalendarProvider` in `providers/`: an object-safe async trait with `send(to, body)` and an optional `check()`, plus a `factory.rs` that dispatches on `sms_config.provider`. Four adapters ship: `twilio`, `gatewayapi`, `sevenio`, and `webhook` (POSTs `{"to","text","sender"}` to any URL, optionally HMAC-signed like the meeting webhook, so a gateway with no adapter is a small script away).

Adapters own request building **and** response parsing, because "the credentials are wrong" looks different on every gateway: HTTP 401 on Twilio, HTTP 401 with a `{"code": "0x0213"}` body on GatewayAPI, and HTTP **200** carrying `{"success": "900"}` on seven.io. Each maps onto `SmsError` (`Auth`, `InvalidRecipient`, `InvalidSender`, `InsufficientCredit`, `RateLimited`, `Transport`, `Other`) so the admin panel and the logs read the same whichever gateway is configured. `parse_error`/`parse_response` are pure functions, unit-tested against the payloads in each vendor's docs, no mock server involved.

**Config** (migration 062, `sms_config`): a singleton row with `provider`, `api_key` (non-secret identifier, Twilio's Account SID; NULL elsewhere), `api_secret_enc` (AES-256-GCM), `sender`, `base_url` (region or self-hosted endpoint; the target URL for `webhook`), `default_country_code`, `enabled`. The `CALRS_SMS_*` block (`PROVIDER`, `API_KEY`, `API_SECRET`, `SENDER`, `BASE_URL`, `DEFAULT_COUNTRY_CODE`) overrides the DB with the same "full block wins" semantics as `CALRS_SMTP_*`, and locks the admin form when active. `factory::validate_config()` is the single gate used by the admin form, the env block, and the read path, so a row that cannot send is treated as "not configured" rather than failing inside a booking request.

**Twilio trial mode:** trial accounts refuse custom bodies, so `Body` must carry the name of a Twilio-provided template. `CALRS_SMS_TWILIO_TRIAL=true` substitutes `sms_appointment_reminders` for the composed message, which makes the Twilio path testable without a paid account: same request shape, same response, same `SendReceipt` parsing, all four events exercised against the real API. Confined to `twilio.rs` (`request_body()`, a pure function with a unit test); the trait, `factory.rs`, and `message.rs` know nothing about it. Environment-only on purpose, with no `sms_config` column and no admin field, since an operator who could flip it from the panel would ship canned templates to real guests. Read on its own rather than as part of the all-or-nothing `CALRS_SMS_*` block, so it composes with a DB-stored config. Warns on every send and shows a badge in the admin SMS card, because with the template substituted all four events look identical on the handset. The credential check (`check()`, the free path behind "Test gateway") reads the account's `type` and refuses when the flag is set on a `Full` account: that is the only direction that costs money, since on a paid account the template name is just text and every guest would be texted the literal string at full price. The reverse mistake fails closed at send time with Twilio's own refusal. `CALRS_SMS_<PROVIDER>_<OPTION>` is the shape for future gateway-specific extras. (Idea and field testing: Chr1s16, #180.)

**Admin UI:** one SMS card, not one per vendor. A provider `<select>` plus four inputs whose labels, hints, and required-ness come from `factory::PROVIDER_SPECS`; hidden blocks are `disabled` so only the selected gateway's values are submitted. "Test gateway" sends a real message, or verifies the credentials for free when the recipient is left empty and the gateway has a check endpoint (Twilio `GET /Accounts/{SID}.json`, seven.io `GET /api/balance`). Adding a gateway means one adapter file and one `ProviderSpec` entry: no template change.

**What gets sent:** guest-facing only, on four events (`SmsEvent`): `Confirmed` (or, for `requires_confirmation` event types, when the host approves), `Cancelled` (host or guest), `Rescheduled`, and `Reminder` (the `run_reminder_loop` background task). Hosts have no phone field. Sends are best-effort and inline, like the emails they accompany: failures are logged and never block a booking. The reminder loop no longer skips its batch when SMTP is unconfigured, so an SMS-only deployment still gets reminders out.

**Bodies** are composed in `message.rs` from the `sms-*` Fluent keys in the booking's own language, never in the provider. SMS is billed per 160-character GSM-7 segment, and one character outside GSM-7 (a Polish `ł`, a curly quote) drops that to 70, so keep those keys terse: `estimate_segments()` backs a test asserting every shipped body in every shipped language stays within two segments. Host-controlled event titles are shortened to 60 characters so the date and time always survive.

**Phone numbers:** the guest types whatever they like (`06 12 34 56 78`, `0033612345678`, `+33 6 12 34 56 78`); `phone::normalize()` converts to E.164 server-side using the configured default country code, and `bookings.guest_phone` is E.164 from then on. This is not libphonenumber: it handles trunk and international prefixes and leaves real validity to the gateway. Numbers are shown to the host on the bookings dashboard and never to other guests.

**Phone modes** (`event_types.sms_phone_mode`, three states rather than a boolean, same shape Cal.com converged on): `off` shows no field; `optional` shows one and says plainly that an empty answer means no SMS, so the guest is never silently dropped from a channel they expected; `required` enforces it, for event types where the message is the point. `book.html` renders the field through the vendored intl-tel-input widget (see below), so a bad number is an inline field error rather than a full-page one; `resolve_guest_phone()` is the server-side backstop and returns the localised (title, message) pair to render.

**Country picker** (`assets/intl-tel-input/`, vendored at 25.3.1): the phone field is an intl-tel-input widget with a flag picker, format-as-you-type, and libphonenumber validation. All seven files are baked into the binary and served from `/static/intl-tel-input/` by one allowlist handler, so a booking page makes no third-party request; `utils.js` is 265 KB of libphonenumber and is lazy-loaded via `loadUtils` rather than linked on every page. Two licences apply, MIT for the widget and Apache-2.0 for `utils.js`, both recorded in `assets/intl-tel-input/README.md` and pinned by a test.

The country is **seeded, not guessed**. Only an explicit BCP-47 region subtag counts (`fr-FR`, `pt-BR`); otherwise the seed is `sms_config.default_country_code` mapped to a country through the widget's own data. A bare language tag is deliberately ignored, because a language is not a country: `sv` is Swedish but reads as El Salvador, `pt` would send Brazilian guests to Portugal, and `uk` is Ukrainian rather than the United Kingdom. The dial-code lookup honours the `priority` field, without which the name-sorted list resolves `+1` to American Samoa and `+44` to Guernsey. Since the flag is visible and editable, a wrong seed is a visible default rather than a silent rewrite. The visible input keeps `name="phone"`, so with no JavaScript the field still posts and `phone::normalize()` still resolves it against the configured country.

**Spend controls.** SMS is the one feature where a public, unauthenticated form spends real money on a recipient the guest chooses, which is the SMS pumping (AIT) attack. Three layers:
- **Who may enable it**: `auth_config.sms_allow_all_users`, off by default, so only admins can put an event type into an SMS mode (same reasoning as shared resources). `can_enable_sms()` and `resolve_phone_mode()` enforce it server-side; a user who may not change it has the stored value carried forward, so a member editing an admin's event type cannot turn SMS off either.
- **How much**: `sms_config.daily_cap` (0 = unlimited) checked in `notify_guest()` against `sms_usage`, which records segments and cost per accepted message and deliberately stores no recipient number. Over the cap, calrs stops texting and email carries on: a booking must never fail because the SMS budget ran out. The admin panel shows today's count and cost.
- **Outside calrs**: the admin card tells the operator to restrict destination countries at the gateway (Twilio calls this Geo Permissions), keep the account prepaid without auto-recharge, and leave the captcha on. The panel warns when SMS is configured and the captcha is not, since that combination is the actual open relay.

**Guest ICS:** `GET /booking/ics/{cancel_token}` serves the booking as `text/calendar`, and `confirmed.html` links it as "Add to calendar". The confirmation email already attaches one, but the guest is looking at the page at the moment they want to add it, and an instance with no SMTP never sends that email at all.

---

## Known issues & TODOs

### Security
- ~~**CalDAV/SMTP passwords** stored as hex-encoded plaintext~~ — **Fixed in v0.10.0**: passwords are now encrypted at rest using AES-256-GCM. Key is auto-generated at `$DATA_DIR/secret.key` or provided via `CALRS_SECRET_KEY` env var. Legacy hex-encoded passwords are auto-migrated on startup.
- ~~**Passwords echoed to terminal**~~ — **Fixed in v0.10.0**: `prompt_password()` now uses `rpassword` for hidden input.

### Features not yet implemented
- REST API for third-party integrations (tracked in #169)

**Note:** delta sync is implemented. `commands/sync.rs` stores a per-calendar `sync-token` (migration 027) and `ctag`, issues an RFC 6578 `sync-collection` REPORT when it has one, and falls back to a full fetch when the server rejects it or reports an empty delta against a changed ctag. `--full` clears both. EWS sources keep their own flow.

### Test coverage roadmap
- **Web handler integration tests** — use `axum::test` with in-memory SQLite to test the full booking flow (create event type → fetch slots → book → confirm/cancel), dashboard renders, admin panel, token-based actions. Requires building a shared test harness (DB seed, AppState setup). This is the biggest coverage opportunity (~49% of codebase is `web/mod.rs`).
- **CLI command tests** (`commands/*.rs`) — unit tests for `sync.rs`, `booking.rs`, `event_type.rs`, `source.rs`, `config.rs`, `user.rs`. These are I/O-heavy (DB + CalDAV) so they need mock/in-memory DB fixtures. Can reuse the same test harness from the web handler tests.

---

## Deployment

calrs listens on HTTP (port 3000 by default). In production, use a reverse proxy for TLS:

- **Caddy** — simplest: `cal.example.com { reverse_proxy localhost:3000 }` (automatic HTTPS)
- **Nginx** — `proxy_pass http://127.0.0.1:3000` with `X-Forwarded-For`, `X-Forwarded-Proto`, `Host` headers

`CALRS_BASE_URL` must be set to the public URL (e.g. `https://cal.example.com`) for OIDC redirects and email links (including approve/decline buttons).

---

## Build & run

```bash
cargo build --release

# Create an admin user
./target/release/calrs user create --email alice@example.com --name "Alice" --admin

# Add a Nextcloud CalDAV source
./target/release/calrs source add \
  --url https://nextcloud.example.com/remote.php/dav \
  --username alice@example.com \
  --name "Nextcloud"

# Sync events
./target/release/calrs sync

# Create a 30-minute meeting type
./target/release/calrs event-type create \
  --title "30min intro call" \
  --slug intro \
  --duration 30

# View availability for next 7 days
./target/release/calrs event-type slots intro

# View your calendar
./target/release/calrs calendar show --from 2025-01-01 --to 2025-01-14
```

Data is stored at `$XDG_DATA_HOME/calrs/calrs.db` (typically `~/.local/share/calrs/calrs.db` on Linux). Override with `--data-dir` flag or `CALRS_DATA_DIR` env var.

---

## Development notes

- Run tests: `cargo test`
- Check without building: `cargo check`
- Lint: `cargo clippy -- -D warnings`
- Format: `cargo fmt`

### Coding style (enforced by pre-commit hook)

**Always run `cargo fmt` on any modified Rust file before committing.** The pre-commit hook runs `cargo fmt --check` and will reject unformatted code. When editing Rust code, write it in `rustfmt`-canonical style from the start — in particular, if a function call with arguments fits on one line after formatting, don't split it across multiple lines.

### Known compiler warnings (intentional)

The following `dead_code` warnings are expected and should **not** be suppressed:

- **`models.rs` structs** (`Account`, `Group`, `CaldavSource`, `Calendar`, `Event`, `EventType`, `AvailabilityRule`, `AvailabilityOverride`, `Booking`) — Domain model definitions kept for documentation and future use. All current DB queries use tuple destructuring via `sqlx::query_as` instead. These structs will be used when migrating to typed queries.
- **`auth.rs` `cleanup_expired_sessions()`** — Session cleanup utility not yet wired into a scheduled task. Will be used when adding periodic maintenance (e.g. on startup or via a background task).
- **`caldav/mod.rs` `RawEvent.href` field** — Set during CalDAV fetch but not yet read. Kept for potential future use in delta sync.

**Tests that touch `CALRS_*` environment variables** must take `crate::test_support::ENV_LOCK` and hold a `crate::test_support::EnvGuard`. Environment variables are process-global and Rust runs tests in parallel threads inside one process, so a module-local lock is not enough. `config_general_set_and_clear` failed intermittently for exactly that reason: `commands::config` had its own lock while `caldav` set the same variable from another module.

When adding a new migration:
1. Create `migrations/NNN_description.sql` with the DDL.
2. **CRITICAL: Register it in `src/db.rs`** in the `migrations` array inside `migrate()`. Forgetting this step means the migration never runs on existing deployments, and any queries referencing the new table/column will fail silently (due to `unwrap_or_default()`). This has caused production bugs before — always verify the migration is registered.

### Localization (Fluent)

calrs ships with translations for English, French, Spanish, Polish, German, Italian, Estonian and Brazilian Portuguese. Source files live under `i18n/{lang}/main.ftl` and are embedded in the binary via `include_str!` (no runtime files). The loader, language detection, and minijinja `t()` global are in `src/i18n.rs`. Templates use `{{ t("message-id", arg=value) }}` and the active language is injected into the rendering context as `lang` by the calling handler.

Both the guest side and the host side (dashboard, settings, forms, admin panel, auth pages) render through Fluent, including the bare error responses the booking flow returns without page chrome. **All eight locales are complete at 1043 keys**, held there by a test; the per-key English fallback still exists but nothing currently uses it.

Three helpers stay English on purpose: the CSRF rejection, the 500 page, and the OIDC failure. They live in helpers called from ~240 sites with no `lang` in scope, and they are diagnostics rather than flow messages.

All locales address the reader informally, matching what the earliest translations chose: du, tu, tú, ty, você, sa. The captcha strings in German are the one leftover in the formal register.

**Where `lang` comes from on host pages.** The `AuthUser`, `AdminUser` and `OptionalAuthUser` extractors resolve it once, in `src/auth.rs`, from the user's saved preference then `Accept-Language`. A dashboard handler passes `lang => auth_user.lang` and nothing else. Pre-login pages (login, register) have no user row, so they call `i18n::detect_from_headers` directly.

**Keys that reach Fluent through a variable.** Most call sites name their key inline, as `translate(lang, "key", None)`. Two places on the booking path do not: `render_booking_action_error_keys(state, headers, title_key, body_key)` takes a `bae-*` pair, and the booking-form validators return their key as `Err("validate-*")` for the handler to resolve. Both forms are covered by the Rust key guard, so a typo still fails the build rather than rendering the raw id to a guest. Add any further indirection to `find_rust_keys` in `src/i18n.rs` at the same time you add the indirection itself.

**Guard tests** in `src/i18n.rs`: every `t()` key referenced from a template exists in the English bundle; the same for keys used from Rust (including the two indirect forms above); every template still loads; every locale covers every English key; and plural messages carry the categories the locale's grammar needs. That last one matters because Polish selects one/few/many for integers, so a translation that copies English's one/other reads wrong at 2 and at 5. Two further guards in `src/web/mod.rs`: one fails if a host-facing template is rendered without a `lang` in its context, the other fails if any call site hands the booking error page a bare English sentence instead of a key or an already-translated value.

**Numbers passed to `t()`** reach Fluent as numbers, not strings, so `{ $count -> [one] ... }` plural selectors work. Grouping is switched off, so an integer renders as it always did ("1440", not "1,440").

**Strings a page composes at runtime** (JS building a summary hint or a search result) cannot use Fluent arguments, because the values only exist after the visitor acts. Those keys use `%1`/`%2` placeholders substituted client-side, collected in one object per page (`ETF_I18N`, `ADMIN_I18N`) built with `{{ t('key') | tojson }}`. Prefer real Fluent arguments everywhere else.

**Literal braces in a Fluent value** must be escaped as `{"{"}`: a bare `{` starts a placeable. This bites the meeting-pattern help, which documents `{username}` and `{random}` tokens.

**Branch workflow (long-lived `i18n` branch).** The `i18n` branch is permanent. **Do not delete it after merging.** Translation contributions arrive as pull requests against it. Periodically (e.g. before each release) merge `i18n` into `main`, then continue using the same branch for the next round. The branch never gets recreated, and it normally sits exactly at `main`, so a round never starts stale.

**There is no translation platform.** A Hosted Weblate project was applied for and never approved, so the account was closed and the links 404ed for months while the README still advertised them (#200). No commit in this repository's history was ever authored by Weblate. Every community translation calrs has received came in as an ordinary pull request. Do not reintroduce a platform reference without a working project behind it.

**When you add or change a translatable string:**
1. Land it on the `i18n` branch first, not `main`. This keeps half-translated UI off `main` and gives contributors a window before the next merge.
2. Add the new key to `i18n/en/main.ftl` (the source of truth), then to every other locale. The runtime still falls back to English per missing key, but the coverage test does not let you rely on it.
3. If the change touches a template that wasn't translated yet, convert its hard-coded strings to `{{ t("...") }}` calls in the same commit, and add render-site context entries (`lang => crate::i18n::detect_from_headers(&headers)` for guest pages, `lang => auth_user.lang` for authenticated dashboard pages).
4. Run `cargo test i18n::` before pushing: it checks coverage across all eight locales and the plural categories each language needs.

**Adding a key now costs eight translations, not one.** The coverage test fails until every locale has a value. That is deliberate: a half-translated page is worse than an English one because nobody notices it is wrong. If you cannot supply all eight, land the key on `i18n` and leave it there until someone can.

**When you add a new locale**, in this order:
1. Create `i18n/{code}/main.ftl` and translate every key. **Not empty**: the moment the locale is registered, `every_locale_covers_every_english_key` demands all of them, so an empty file fails with over a thousand errors.
2. Register it in `SUPPORTED_LANGS` in `src/i18n.rs`: code, the language's own name for itself, `include_str!`. `supported_with_labels()` derives the settings dropdown from this tuple, so there is no second place to edit.
3. Add the locale's CLDR plural categories to the `required` table in `plural_messages_carry_the_locale_categories`. Most need `one, other`; Polish needs `one, few, many`.
4. `cargo test i18n::`, then push to `i18n`.

**Anti-patterns to avoid:**
- Don't bypass `t()` and inline new English strings directly in templates that are already translated. The other locales silently drift out of date.
- Don't merge `main` into `i18n` to "sync"; the flow goes `i18n → main`. New features that touch UI text should branch off `i18n`, not `main`.

### Updating the GitHub Pages site

The site (landing page + mdbook docs) lives on the `gh-pages` branch. To update it:

1. **Sync the branch first:** `git fetch origin gh-pages` and work from `origin/gh-pages`, not the local ref. **The local `gh-pages` is almost always many releases stale** — nothing on `main` ever advances it, so it sits wherever it was left the last time the site was touched on this machine. Editing the stale copy produces a page that re-adds features the published site already documents, and the push is rejected as non-fast-forward. `git worktree add <dir> gh-pages` inherits the same staleness; reset to `origin/gh-pages` inside it before touching anything.
2. **Build the docs on `main`:** `mdbook build docs` (output goes to `docs/book/`)
3. **Switch branch:** `git checkout gh-pages`
4. **Copy docs source and rebuild:** `git checkout main -- docs/src docs/book.toml` then `mdbook build docs`
5. **Replace published docs:** `cp -r docs/book/* docs/` then `rm -rf docs/src docs/book.toml docs/book`
6. **Drop orphaned build artefacts:** mdbook writes content-hashed `searchindex-*.js` / `toc-*.js`, and the copy in step 5 never removes the previous hash. They accumulate silently. Delete any hashed file no HTML/JS/CSS under `docs/` still references.
7. **Update `index.html`** if the landing page needs changes (feature cards, version badge in the hero, test count)
8. **Stage only `docs/` and `index.html`** — do not stage untracked files from main (worktrees, build artifacts)
9. **Commit with `--no-verify`** — the pre-commit hook expects `Cargo.toml` which doesn't exist on `gh-pages`
10. **Push:** `git push origin gh-pages`
11. **Switch back:** `git checkout main`

When adding a new subcommand:
1. Create `src/commands/yourcmd.rs` with a `YourCommands` enum and `pub async fn run(db, cmd)`.
2. Add `pub mod yourcmd;` to `src/commands/mod.rs`.
3. Add the variant to the `Commands` enum in `src/main.rs`.
4. Wire it in the `match` block in `main()`.
