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
│   └── 005_requires_confirmation.sql ← requires_confirmation on event_types
├── templates/
│   ├── base.html                 ← base layout + CSS
│   ├── auth/
│   │   ├── login.html            ← login page (local + SSO button)
│   │   └── register.html         ← registration page
│   ├── dashboard.html            ← user dashboard (event types, bookings)
│   ├── admin.html                ← admin dashboard (users, auth, OIDC, SMTP)
│   ├── event_type_form.html      ← create/edit event types
│   ├── profile.html              ← public user profile
│   ├── slots.html                ← available time slots
│   ├── book.html                 ← booking form
│   └── confirmed.html            ← confirmation page
└── src/
    ├── main.rs                   ← CLI entry point, Cli/Commands enum, tokio main
    ├── db.rs                     ← SQLite pool setup (WAL mode) + migration runner
    ├── models.rs                 ← domain structs: Account, User, Session, AuthConfig,
    │                               CaldavSource, Calendar, Event, EventType, Booking
    ├── auth.rs                   ← authentication: password hashing, sessions, OIDC,
    │                               axum extractors (AuthUser, AdminUser), web handlers
    ├── email.rs                  ← SMTP email with .ics calendar invites
    ├── caldav/
    │   └── mod.rs                ← CalDAV client: discovery, calendar list, event fetch
    ├── web/
    │   └── mod.rs                ← Axum web server: dashboard, booking, admin panel
    └── commands/
        ├── mod.rs                ← re-exports all subcommands
        ├── init.rs               ← `calrs init` — first-time account setup
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
- **`caldav_sources`** — CalDAV server connections (URL, credentials, sync state). `enabled` flag, `ON DELETE CASCADE`
- **`calendars`** — calendar collections discovered under a source; `is_busy=1` means events block availability
- **`events`** — cached remote events from CalDAV sync; `uid` is UNIQUE, stores `raw_ical`, `etag`, `rrule`, `all_day`, `timezone`
- **`event_types`** — bookable meeting templates (slug unique per account, `duration_min`, `buffer_before`/`buffer_after`, `min_notice_min`, `location_type`/`location_value`, `requires_confirmation`)
- **`availability_rules`** — weekly recurring windows per event type (day_of_week 0=Sun…6=Sat, HH:MM times)
- **`availability_overrides`** — date-specific exceptions (day off, special hours). `is_blocked` flag
- **`bookings`** — bookings with `uid` (iCal), guest info, status (confirmed/pending/cancelled), `cancel_token`/`reschedule_token`
- **`smtp_config`** — SMTP server settings (host, port, credentials, sender), one per account
- **`groups`** / **`user_groups`** — group system (schema exists, not yet wired up)

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

**URL resolution:** All hrefs from the server are resolved via `resolve_url()` which uses the server origin (scheme + host), not the base URL path, to avoid path duplication.

**XML templates** are `const &str` at the bottom of the file (PROPFIND_PRINCIPAL, PROPFIND_CALENDAR_HOME, PROPFIND_CALENDARS, REPORT_CALENDAR_DATA).

**Timeouts:** 10s default for discovery/metadata requests, 60s for event fetches (calendars can have thousands of events).

**Tested with:** BlueMind (4000+ events). Handles both `aic:` and `x1:` namespace prefixes for calendar colors, `cso:` and `cs:` for ctags.

**Known limitation:** The XML parser is a simple string-based tag extractor. It works for well-formed CalDAV responses but is not robust against malformed or deeply nested XML. A future improvement would be to use `quick-xml` + serde derive.

**iCal parsing:** The `extract_ical_field()` function in `sync.rs` extracts fields from the VEVENT block only (skips VTIMEZONE to avoid matching wrong DTSTART). Dates are stored as-is from iCal: `YYYYMMDD` for all-day events, `YYYYMMDDTHHMMSS` for timed events.

---

## Authentication & authorization

File: `src/auth.rs`

**Local auth:** Argon2 password hashing. Server-side sessions stored in SQLite with 30-day TTL. HttpOnly cookies (`calrs_session`).

**OIDC:** OpenID Connect via `openidconnect` 4.x crate. Authorization code flow with PKCE (S256). State, nonce, and PKCE verifier stored in short-lived cookies during the flow. Tested with Keycloak.

**User linking:** On OIDC callback, tries: (1) match by `oidc_subject`, (2) match by email (links existing local user), (3) auto-register if enabled.

**Extractors:** `AuthUser` (redirects to login if not authenticated), `AdminUser` (returns 403 if not admin). Both implemented as axum `FromRequestParts`.

**URL scheme:** User-scoped public booking URLs: `/u/{username}/{slug}`. Legacy single-user routes (`/{slug}`) kept for backward compatibility.

---

## Web UI

File: `src/web/mod.rs`, templates in `templates/`

**Dashboard** (`/dashboard`): Lists event types (create/edit/toggle/view), pending bookings (confirm/decline), upcoming bookings (cancel with optional reason).

**Admin panel** (`/dashboard/admin`): User management (promote/demote, enable/disable), auth settings (registration toggle, allowed domains), OIDC config, SMTP status. Requires `AdminUser`.

**Public pages:** User profile (`/u/{username}`), time slot picker, booking form, confirmation page. Event types support location (video link, phone, in-person, custom).

**Email notifications:** Booking confirmation, cancellation, pending notice, approval request — all with `.ics` calendar invite attachments. Location included in emails and ICS.

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

### Correctness
- **Timezone handling is incomplete.** Availability slot computation operates naively on local time strings.
- **Recurrence rules (RRULE) not expanded.** Recurring events won't block availability correctly yet.

### Features not yet implemented
- Group sync from OIDC provider (Keycloak)
- Group-based event type permissions
- CalDAV write-back (push bookings to user's calendar)
- Delta sync using CalDAV `sync-token` and `ctag`
- Docker image / systemd unit file

---

## Build & run

```bash
cargo build --release

# First run
./target/release/calrs init

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

When adding a new subcommand:
1. Create `src/commands/yourcmd.rs` with a `YourCommands` enum and `pub async fn run(db, cmd)`.
2. Add `pub mod yourcmd;` to `src/commands/mod.rs`.
3. Add the variant to the `Commands` enum in `src/main.rs`.
4. Wire it in the `match` block in `main()`.
