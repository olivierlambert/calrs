# Architecture

## Project structure

```
calrs/
├── Cargo.toml              Package manifest
├── Dockerfile              Multi-stage Docker build
├── calrs.service           systemd unit file
├── migrations/             SQLite schema (incremental)
│   ├── 001_initial.sql     Core tables
│   ├── 002_auth.sql        Users, sessions, auth config
│   ├── 003_username.sql    Username support
│   ├── 004_oidc.sql        OIDC columns
│   ├── 005_requires_confirmation.sql
│   ├── 006_group_event_types.sql
│   └── 007_caldav_write.sql
├── templates/              Minijinja HTML templates
│   ├── base.html           Base layout + CSS (dark mode)
│   ├── auth/               Login, registration
│   ├── dashboard.html      User dashboard
│   ├── admin.html          Admin panel
│   ├── source_form.html    Add CalDAV source
│   ├── event_type_form.html  Create/edit event types
│   ├── slots.html          Slot picker (timezone-aware)
│   ├── book.html           Booking form
│   └── confirmed.html      Confirmation page
├── docs/                   mdBook documentation
└── src/
    ├── main.rs             CLI entry point (clap)
    ├── db.rs               SQLite connection + migrations
    ├── models.rs           Domain types
    ├── auth.rs             Authentication (local + OIDC)
    ├── email.rs            SMTP email with .ics invites
    ├── caldav/mod.rs       CalDAV client (RFC 4791)
    ├── web/mod.rs          Axum web server + handlers
    └── commands/           CLI subcommands
        ├── init.rs
        ├── source.rs
        ├── sync.rs
        ├── calendar.rs
        ├── event_type.rs
        ├── booking.rs
        ├── config.rs
        └── user.rs
```

## Database

**SQLite** in WAL mode. Single file, zero ops. Foreign keys with `ON DELETE CASCADE`.

### Core tables

| Table | Purpose |
|---|---|
| `accounts` | User profiles (name, email, timezone) |
| `users` | Authentication (password hash, role, username) |
| `sessions` | Server-side sessions |
| `caldav_sources` | CalDAV server connections |
| `calendars` | Discovered calendars |
| `events` | Synced calendar events |
| `event_types` | Bookable meeting templates |
| `availability_rules` | Per-event-type availability (day + time range) |
| `bookings` | Guest bookings |
| `smtp_config` | SMTP settings |
| `auth_config` | Registration, OIDC settings |
| `groups` | OIDC groups |
| `user_groups` | Group membership |

## Web server

**Axum 0.8** with `Arc<AppState>` shared state containing the `SqlitePool` and `minijinja::Environment`.

### Route structure

| Route | Handler |
|---|---|
| `/auth/login`, `/auth/register` | Authentication |
| `/auth/oidc/login`, `/auth/oidc/callback` | OIDC flow |
| `/dashboard` | User dashboard |
| `/dashboard/admin` | Admin panel |
| `/dashboard/event-types/*` | Event type CRUD |
| `/dashboard/sources/*` | CalDAV source management |
| `/dashboard/bookings/*` | Booking actions |
| `/u/{username}` | Public user profile |
| `/u/{username}/{slug}` | Public slot picker |
| `/u/{username}/{slug}/book` | Booking form + submit |
| `/g/{group_slug}/{slug}` | Group booking pages |

## CalDAV client

Minimal RFC 4791 implementation:

- **PROPFIND** — principal discovery, calendar-home-set, calendar listing
- **REPORT** — event fetch (calendar-query)
- **PUT** — write events to calendar
- **DELETE** — remove events from calendar
- **OPTIONS** — connection test

Handles absolute and relative hrefs, BlueMind/Apple namespace prefixes, tags with attributes.

## Templates

**Minijinja 2** with file-based loader. Templates extend `base.html` which provides:

- CSS custom properties for theming
- Dark mode via `prefers-color-scheme`
- Responsive layout
- No JavaScript framework — vanilla JS only where needed (timezone detection, provider presets)

## Email

**Lettre** for SMTP with STARTTLS. ICS generation is hand-crafted (no icalendar crate dependency for generation):

- `METHOD:REQUEST` for confirmations
- `METHOD:CANCEL` for cancellations
- Events include `ORGANIZER`, `ATTENDEE`, `LOCATION`, `STATUS`

## Authentication flow

### Local
1. Registration/login form → POST with email + password
2. Password verified with Argon2
3. Session created in SQLite → session ID in HttpOnly cookie
4. Extractors (`AuthUser`, `AdminUser`) validate session on each request

### OIDC
1. User clicks "Sign in with SSO"
2. Redirect to OIDC provider with PKCE challenge
3. Provider redirects back with authorization code
4. calrs exchanges code for tokens
5. Extracts email, name, groups from ID token
6. Links to existing user by email or creates new user
7. Session created as with local auth

## Dependencies

Key crates:

| Crate | Purpose |
|---|---|
| `clap` | CLI argument parsing |
| `axum` | Web framework |
| `sqlx` | Async SQLite |
| `reqwest` | HTTP client (CalDAV) |
| `minijinja` | HTML templating |
| `lettre` | SMTP email |
| `chrono` + `chrono-tz` | Time and timezone handling |
| `argon2` | Password hashing |
| `openidconnect` | OIDC client |
| `icalendar` | ICS parsing |
