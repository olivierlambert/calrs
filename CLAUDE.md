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
| HTTP client | `reqwest` (rustls, no openssl) | CalDAV PROPFIND/REPORT requests |
| XML parsing | `quick-xml` 0.31 | CalDAV responses are XML over WebDAV |
| iCal | `icalendar` crate | Parsing/generating VEVENT data |
| Time | `chrono` + `chrono-tz` | Timezone handling is a known complexity area |
| IDs | `uuid` v1 | UUID v4 for all primary keys |
| Terminal output | `colored` + `tabled` | Colored text and ASCII tables in CLI output |
| Web server | `axum` 0.8 | HTTP booking page, served from CLI |
| Templates | `minijinja` 2 | Jinja2-compatible, loaded from `templates/` dir |
| Auth | `argon2` + `password-hash` | Argon2 password hashing for local accounts |
| Auth (OIDC) | `openidconnect` 4.x | OpenID Connect SSO (Keycloak, etc.) with PKCE |
| Sessions | `axum-extra` (cookies) | Server-side sessions in SQLite, HttpOnly cookies |
| Email | `lettre` 0.11 | SMTP with STARTTLS, async tokio transport |
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
│   └── 010_confirm_token.sql     ← confirm_token on bookings for email approve/decline
├── templates/
│   ├── base.html                 ← base layout + CSS (light/dark mode)
│   ├── auth/
│   │   ├── login.html            ← login page (local + SSO button)
│   │   └── register.html         ← registration page
│   ├── dashboard.html            ← user dashboard (event types, bookings)
│   ├── admin.html                ← admin dashboard (users, auth, OIDC, SMTP)
│   ├── event_type_form.html      ← create/edit event types (with group selector)
│   ├── source_form.html          ← add CalDAV source (provider presets)
│   ├── source_test.html          ← connection test / sync results
│   ├── profile.html              ← public user profile
│   ├── group_profile.html        ← public group page
│   ├── slots.html                ← available time slots (with timezone picker)
│   ├── book.html                 ← booking form
│   ├── confirmed.html            ← confirmation / pending page
│   ├── troubleshoot.html         ← availability troubleshoot timeline
│   ├── booking_approved.html     ← token-based approve success page
│   ├── booking_decline_form.html ← token-based decline form (optional reason)
│   ├── booking_declined.html     ← token-based decline success page
│   └── booking_action_error.html ← error page for invalid/expired tokens
└── src/
    ├── main.rs                   ← CLI entry point, Cli/Commands enum, tokio main
    ├── db.rs                     ← SQLite pool setup (WAL mode) + migration runner
    ├── models.rs                 ← domain structs: Account, User, Session, AuthConfig,
    │                               CaldavSource, Calendar, Event, EventType, Booking
    ├── auth.rs                   ← authentication: password hashing, sessions, OIDC,
    │                               axum extractors (AuthUser, AdminUser), web handlers
    ├── email.rs                  ← SMTP email with .ics calendar invites, HTML templates
    ├── rrule.rs                  ← RRULE expansion (DAILY/WEEKLY/MONTHLY, EXDATE, BYDAY)
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
        └── user.rs               ← `calrs user create/list/promote/set-password`
```

---

## Database schema (SQLite)

Migrations are tracked via `_migrations` table and run incrementally at startup via `db::migrate()`.

Key tables:

- **`users`** — multi-user: email, name, password_hash (argon2), role (admin/user), auth_provider (local/oidc), oidc_subject, username (unique), enabled flag
- **`sessions`** — server-side sessions: token (PK), user_id, expires_at (30-day TTL)
- **`auth_config`** — singleton: registration_enabled, allowed_email_domains, OIDC settings (issuer, client_id, client_secret, auto_register)
- **`accounts`** — scheduling accounts linked to users via `user_id`
- **`caldav_sources`** — CalDAV server connections (URL, credentials, sync state, `write_calendar_href`). `enabled` flag, `ON DELETE CASCADE`
- **`calendars`** — calendar collections discovered under a source; `is_busy=1` means events block availability
- **`events`** — cached remote events from CalDAV sync; unique on `(uid, COALESCE(recurrence_id, ''))`, stores `raw_ical`, `etag`, `rrule`, `all_day`, `timezone`, `recurrence_id`, `status`
- **`event_types`** — bookable meeting templates (slug unique per account, `duration_min`, `buffer_before`/`buffer_after`, `min_notice_min`, `location_type`/`location_value`, `requires_confirmation`, `group_id`, `created_by_user_id`)
- **`availability_rules`** — weekly recurring windows per event type (day_of_week 0=Sun…6=Sat, HH:MM times)
- **`availability_overrides`** — date-specific exceptions (day off, special hours). `is_blocked` flag
- **`bookings`** — bookings with `uid` (iCal), guest info, status (confirmed/pending/cancelled/declined), `cancel_token`/`reschedule_token`/`confirm_token`, `assigned_user_id` (for group round-robin), `caldav_calendar_href` (write-back tracking)
- **`smtp_config`** — SMTP server settings (host, port, credentials, sender), one per account
- **`groups`** / **`user_groups`** — group system synced from Keycloak OIDC; groups have `slug` for public URLs

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

**User linking:** On OIDC callback, tries: (1) match by `oidc_subject`, (2) match by email (links existing local user), (3) auto-register if enabled.

**Extractors:** `AuthUser` (redirects to login if not authenticated), `AdminUser` (returns 403 if not admin). Both implemented as axum `FromRequestParts`.

**Login/register redirect:** If the user is already authenticated, visiting `/auth/login` or `/auth/register` redirects to `/dashboard` instead of showing the form.

**URL scheme:** User-scoped public booking URLs: `/u/{username}/{slug}`. Legacy single-user routes (`/{slug}`) kept for backward compatibility.

---

## Web UI

File: `src/web/mod.rs`, templates in `templates/`

**Dashboard** (`/dashboard`): Lists personal and group event types (create/edit/toggle/view), calendar sources (add/test/sync/remove), pending bookings (confirm/decline), upcoming bookings (cancel with optional reason).

**Admin panel** (`/dashboard/admin`): User management (promote/demote, enable/disable), auth settings (registration toggle, allowed domains), OIDC config, SMTP status, groups overview, impersonation. Requires `AdminUser`.

**Public pages:** User profile (`/u/{username}`), group profile (`/g/{group-slug}`), time slot picker (with timezone selector), booking form, confirmation page. Event types support location (video link, phone, in-person, custom).

**Availability troubleshoot** (`/dashboard/troubleshoot/{event_type_id}`): Visual timeline showing why slots are available or blocked, with event details. Helps debug availability issues.

**Group event types:** Created under a group from the dashboard. Combined availability shows slots where ANY group member is free. Round-robin assignment picks the least-busy available member. Public URLs: `/g/{group-slug}/{slug}`.

**Timezone support:** Guest timezone picker on slot pages. Browser timezone auto-detected via `Intl.DateTimeFormat`. Times displayed and booked in the guest's selected timezone.

**Admin impersonation:** Admins can impersonate any user from the admin panel to troubleshoot their view. Uses a separate `calrs_impersonate` cookie.

**Email approve/decline:** Pending bookings generate a `confirm_token`. Host notification emails include Approve/Decline buttons linking to `/booking/approve/{token}` and `/booking/decline/{token}`. These are unauthenticated public endpoints. Requires `CALRS_BASE_URL` env var.

**Email notifications:** Booking confirmation, cancellation, pending notice, approval request (with action buttons), decline notice — all HTML emails with plain text fallback. Confirmation and cancellation include `.ics` calendar invite attachments. Location included in emails and ICS.

**CalDAV write-back:** Confirmed bookings are pushed to the host's CalDAV calendar (if `write_calendar_href` is configured on the source). On cancellation, the event is deleted from CalDAV.

---

## CLI UX conventions

- Use `colored` for status: `"✓".green()`, `"✗".red()`, `"…".dimmed()`
- Use `tabled` for listing resources (sources, event types, bookings)
- Interactive prompts via `prompt()` / `prompt_with_default()` helpers
- All commands take `&SqlitePool` as first argument

---

## Known issues & TODOs

### Security
- **CalDAV passwords** stored as hex-encoded plaintext in `password_enc`. Plan: use `keyring` or `age` encryption.
- **Passwords echoed to terminal** during `source add`. Replace `prompt()` with `rpassword::read_password()`.

### Features not yet implemented
- Delta sync using CalDAV `sync-token` and `ctag`
- Reschedule flow (change date/time without cancelling)
- Availability overrides (block specific dates, add special hours)
- REST API for third-party integrations

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

### Known compiler warnings (intentional)

The following `dead_code` warnings are expected and should **not** be suppressed:

- **`models.rs` structs** (`Account`, `Group`, `CaldavSource`, `Calendar`, `Event`, `EventType`, `AvailabilityRule`, `AvailabilityOverride`, `Booking`) — Domain model definitions kept for documentation and future use. All current DB queries use tuple destructuring via `sqlx::query_as` instead. These structs will be used when migrating to typed queries.
- **`auth.rs` `cleanup_expired_sessions()`** — Session cleanup utility not yet wired into a scheduled task. Will be used when adding periodic maintenance (e.g. on startup or via a background task).
- **`caldav/mod.rs` `RawEvent.href` field** — Set during CalDAV fetch but not yet read. Kept for potential future use in delta sync.

When adding a new subcommand:
1. Create `src/commands/yourcmd.rs` with a `YourCommands` enum and `pub async fn run(db, cmd)`.
2. Add `pub mod yourcmd;` to `src/commands/mod.rs`.
3. Add the variant to the `Commands` enum in `src/main.rs`.
4. Wire it in the `match` block in `main()`.
