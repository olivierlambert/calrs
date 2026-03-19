# Architecture

## Project structure

```
calrs/
├── Cargo.toml              Package manifest
├── Dockerfile              Multi-stage Docker build
├── calrs.service           systemd unit file
├── migrations/             SQLite schema (35 incremental migrations, see migrations/ dir)
├── templates/              Minijinja HTML templates
│   ├── base.html           Base layout + CSS (light/dark mode)
│   ├── auth/               Login, registration
│   ├── dashboard_base.html Sidebar layout (all dashboard pages extend this)
│   ├── dashboard_overview.html   Overview with stats
│   ├── dashboard_event_types.html Event types listing
│   ├── dashboard_bookings.html   Bookings listing
│   ├── dashboard_sources.html    Calendar sources
│   ├── dashboard_teams.html      Teams listing
│   ├── dashboard_internal.html   Internal/organization event types
│   ├── admin.html          Admin panel
│   ├── settings.html       Profile & settings (avatar, title, bio)
│   ├── event_type_form.html  Create/edit event types
│   ├── invite_form.html    Invite management for private event types
│   ├── source_form.html    Add CalDAV source
│   ├── source_test.html    Connection test / sync results
│   ├── source_write_setup.html Write-back calendar selection
│   ├── team_form.html      Create/edit team
│   ├── team_settings.html  Team settings (members, groups, danger zone)
│   ├── overrides.html      Date overrides per event type
│   ├── troubleshoot.html   Availability troubleshoot timeline
│   ├── profile.html        Public user profile
│   ├── team_profile.html   Public team page
│   ├── slots.html          Slot picker (timezone-aware)
│   ├── book.html           Booking form
│   ├── confirmed.html      Confirmation / pending page
│   ├── booking_approved.html     Token-based approve success
│   ├── booking_decline_form.html Token-based decline form
│   ├── booking_declined.html     Token-based decline success
│   ├── booking_cancel_form.html  Guest self-cancel form
│   ├── booking_cancelled_guest.html Guest self-cancel success
│   ├── booking_host_reschedule.html Host-initiated reschedule
│   ├── booking_reschedule_confirm.html Reschedule confirmation
│   └── booking_action_error.html Invalid/expired token error
├── docs/                   mdBook documentation
└── src/
    ├── main.rs             CLI entry point (clap)
    ├── db.rs               SQLite connection + migrations
    ├── models.rs           Domain types
    ├── auth.rs             Authentication (local + OIDC)
    ├── email.rs            SMTP email with .ics invites + HTML templates
    ├── rrule.rs            RRULE expansion (DAILY/WEEKLY/MONTHLY)
    ├── utils.rs            Shared utilities (iCal splitting/parsing)
    ├── caldav/mod.rs       CalDAV client (RFC 4791) + write-back
    ├── web/mod.rs          Axum web server + handlers
    └── commands/           CLI subcommands
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
| `events` | Synced calendar events (unique on uid + recurrence_id) |
| `event_types` | Bookable meeting templates |
| `availability_rules` | Per-event-type availability (day + time range) |
| `availability_overrides` | Date-specific exceptions (blocked days, custom hours) |
| `bookings` | Guest bookings |
| `booking_invites` | Tokenized invite links for private/internal event types |
| `booking_attendees` | Additional attendees per booking |
| `event_type_calendars` | Per-event-type calendar selection (junction table) |
| `event_type_member_weights` | Per-event-type round-robin priority weights |
| `smtp_config` | SMTP settings |
| `auth_config` | Registration, OIDC, theme settings |
| `groups` | OIDC groups (identity sync from Keycloak) |
| `user_groups` | Group membership |
| `teams` | Unified teams (name, slug, visibility, invite_token) |
| `team_members` | Team membership (role: admin/member, source: direct/group) |
| `team_groups` | Links teams to OIDC groups for automatic member sync |

## Web server

**Axum 0.8** with `Arc<AppState>` shared state containing the `SqlitePool` and `minijinja::Environment`.

### Route structure

| Route | Handler |
|---|---|
| `/auth/login`, `/auth/register` | Authentication (redirects to dashboard if already logged in) |
| `/auth/oidc/login`, `/auth/oidc/callback` | OIDC flow |
| `/dashboard` | Overview with stats |
| `/dashboard/admin` | Admin panel + impersonation |
| `/dashboard/event-types/*` | Event type CRUD |
| `/dashboard/sources/*` | CalDAV source management |
| `/dashboard/bookings/*` | Booking actions (confirm, cancel) |
| `/dashboard/teams/*` | Team CRUD |
| `/dashboard/teams/{id}/settings` | Team settings (members, OIDC groups, danger zone) |
| `/dashboard/organization` | Internal event types + invite link generation |
| `/dashboard/invites/{event_type_id}` | Invite management for private event types |
| `/dashboard/troubleshoot/{id}` | Availability troubleshoot timeline |
| `/booking/approve/{token}` | Token-based booking approval (from email) |
| `/booking/decline/{token}` | Token-based booking decline (from email) |
| `/booking/cancel/{token}` | Guest self-cancellation |
| `/u/{username}` | Public user profile |
| `/u/{username}/{slug}` | Public slot picker |
| `/u/{username}/{slug}/book` | Booking form + submit |
| `/team/{slug}` | Public team page |
| `/team/{slug}/{event-slug}` | Team event type booking |
| `/g/{group-slug}` | Redirects to `/team/{slug}` (legacy) |

### Middleware

| Layer | Purpose |
|---|---|
| `TraceLayer` | Logs every HTTP request (method, path, status, latency) |
| `csrf_cookie_middleware` | Sets `calrs_csrf` cookie on responses for CSRF protection |

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
- No JavaScript framework — vanilla JS only where needed (timezone detection, provider presets, CSRF token injection)

## Email

**Lettre** for SMTP with STARTTLS. All emails are **HTML with plain text fallback** (multipart/alternative). ICS generation is hand-crafted (no icalendar crate dependency for generation):

- `METHOD:REQUEST` for confirmations
- `METHOD:PUBLISH` for guest confirmations (avoids mail server re-invites)
- `METHOD:CANCEL` for cancellations
- Events include `ORGANIZER`, `ATTENDEE`, `LOCATION`, `STATUS`

The approval request email includes Approve and Decline action buttons (table-based layout for email client compatibility). These link to token-based public endpoints that don't require authentication.

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

## Testing

calrs has an automated test suite with 219 tests, run on every push and pull request via [GitHub Actions](https://github.com/olivierlambert/calrs/actions/workflows/ci.yml).

**What's tested:**

| Area | Examples |
|---|---|
| RRULE expansion | DAILY/WEEKLY/MONTHLY recurrence, INTERVAL, UNTIL, COUNT, BYDAY, EXDATE |
| iCal parsing | Multi-VEVENT splitting, field extraction, RECURRENCE-ID handling |
| Timezone conversion | TZID extraction, floating times, UTC suffix, all-day events |
| Email rendering | HTML/plain text output, cancellation attribution (host vs guest), .ics attachments |
| Availability engine | Free/busy computation, buffer times, minimum notice, conflict detection |
| Web server | Rate limiter (allow/block/reset/per-IP isolation) |
| Authentication | Argon2 password hashing roundtrip, hash uniqueness |
| Input validation | Booking name/email/notes/date validation, CSRF token verification |
| ICS regression | UTC timezone suffix, location field integrity, convert_to_utc |

```bash
# Run the full suite
cargo test

# Check formatting and lint
cargo fmt --check
cargo clippy -- -D warnings
```

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
| `tracing` + `tracing-subscriber` | Structured logging |
| `tower-http` | HTTP request tracing (TraceLayer) |
