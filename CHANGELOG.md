# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## Features at a glance

| Feature | Version | Description |
|---|---|---|
| CalDAV sync | 0.1.0 | Connect Nextcloud, BlueMind, Fastmail, iCloud, Google, etc. |
| Availability engine | 0.1.0 | Free/busy computation from availability rules + calendar events |
| Event types | 0.1.0 | Bookable meeting templates with duration, buffers, minimum notice |
| SQLite storage | 0.1.0 | Single-file WAL-mode database, zero ops |
| CLI | 0.1.0 | Full command set: init, source, sync, calendar, event-type, booking |
| Booking with conflict detection | 0.1.2 | Validates against both calendar events and existing bookings |
| Email notifications | 0.1.3 | SMTP emails with `.ics` calendar invites (REQUEST/CANCEL) |
| SMTP configuration | 0.1.3 | `calrs config smtp` — stored in SQLite, optional |
| Web booking page | 0.2.0 | Axum server with slot picker, booking form, confirmation page |
| Server-side slot computation | 0.2.0 | Same availability engine as CLI, exposed via HTTP |
| Local authentication | 0.3.0 | Email/password with Argon2, server-side sessions, HttpOnly cookies |
| User roles | 0.3.0 | Admin/user with extractors, first user becomes admin |
| User management CLI | 0.3.0 | `calrs user create/list/promote/demote/set-password` |
| Registration controls | 0.3.0 | Enable/disable registration, restrict by email domain |
| User-scoped URLs | 0.3.0 | Public pages at `/u/{username}` and `/u/{username}/{slug}` |
| Booking cancellation | 0.3.0 | Cancel from dashboard with optional reason + email notification |
| Pending bookings | 0.3.0 | `requires_confirmation` — host approves/declines from dashboard |
| Web dashboard | 0.3.0 | Event types, pending approvals, upcoming bookings |
| OIDC authentication | 0.3.1 | SSO via Keycloak (authorization code + PKCE, auto-discovery) |
| Admin dashboard | 0.3.1 | User management, auth settings, OIDC config, SMTP status |
| Event type management UI | 0.3.1 | Create/edit from dashboard with availability, location, confirmation |
| Location support | 0.3.1 | Video link, phone, in-person, or custom — in pages, emails, `.ics` |
| OIDC group sync | 0.3.2 | Groups synced from Keycloak `groups` JWT claim on SSO login |
| Group event types | 0.4.0 | Combined availability (any member free) + round-robin assignment |
| Public group pages | 0.4.0 | `/g/{group-slug}` and `/g/{group-slug}/{slug}` |
| Timezone support | 0.4.0 | Guest timezone picker, browser auto-detection, tz-aware booking |
| Calendar source management UI | 0.5.0 | Add/test/sync/remove CalDAV sources from the web dashboard |
| Provider presets | 0.5.0 | BlueMind, Nextcloud, Fastmail, iCloud, Google, etc. with auto-fill |
| Docker image | 0.5.1 | Multi-stage Dockerfile, docker-compose example |
| systemd service | 0.5.1 | Production-ready unit file with security hardening |
| CalDAV write-back | 0.6.0 | Push confirmed bookings to host's calendar, delete on cancel |
| Login rate limiting | 0.6.1 | Per-IP rate limiting on login attempts |
| Secure cookies | 0.6.1 | HttpOnly + Secure flag on all session cookies |
| ICS sanitization | 0.6.1 | Prevents injection in calendar invites |
| RRULE expansion | 0.7.0 | Recurring events block availability (DAILY/WEEKLY/MONTHLY, EXDATE) |
| Availability troubleshoot | 0.7.0 | Visual timeline showing why slots are blocked |
| Duplicate email fix | 0.7.0 | Guest emails use METHOD:PUBLISH to avoid mail server re-invites |
| RECURRENCE-ID handling | 0.7.1 | Modified recurring event instances no longer cause phantom occurrences |
| Admin impersonation | 0.8.0 | Admins can impersonate any user for troubleshooting |
| HTML emails | 0.8.3 | Clean, responsive HTML email notifications with plain text fallback |
| Multi-VEVENT sync | 0.8.4 | Recurring events with modified instances properly synced from CalDAV |
| Email approve/decline | 0.8.5 | Approve or decline pending bookings directly from the notification email |
| Timezone-aware CalDAV events | 0.9.0 | Event times converted from their calendar timezone to host timezone for accurate availability |

## [0.9.0] - 2026-03-10

### Added

- **Timezone-aware CalDAV event handling** — CalDAV events now carry their original timezone through sync, storage, and availability computation
  - New `extract_vevent_tzid()` extracts TZID from iCal DTSTART/DTEND lines (e.g., `DTSTART;TZID=Europe/Paris:...` → `Europe/Paris`, trailing `Z` → `UTC`, no TZID → floating/local)
  - New `convert_event_to_tz()` converts event times from their stored timezone to the host's timezone before busy-time overlap checks
  - `events.timezone` column (already existed but was never populated) is now set during both CLI and web sync
  - All availability computations (slot picker, booking conflict checks, group scheduling, troubleshoot timeline) convert event times to the host's timezone
  - Pre-existing events with `timezone = NULL` are treated as floating (host-local) — fully backward-compatible
  - Invalid or unrecognized TZID strings gracefully degrade to floating (no conversion)
  - All-day events pass through unchanged (no timezone applies)
  - RRULE expansion still happens in the event's own timezone, conversion applied after — correct across DST transitions

### Fixed

- **Cross-timezone availability miscalculation** — an event at 10:00 America/New_York now correctly blocks 16:00 for a Europe/Paris host, instead of incorrectly blocking 10:00

## [0.8.5] - 2026-03-09

### Added

- **Email approve/decline for pending bookings** — host notification emails now include "Approve" and "Decline" buttons that work without logging in
  - Token-based authentication via `confirm_token` on each booking
  - Approve: confirms the booking, pushes to CalDAV, sends guest confirmation email
  - Decline: shows a form for an optional reason, notifies the guest by email
  - Requires `CALRS_BASE_URL` environment variable to generate action URLs
  - Graceful handling of already-processed bookings (already approved, declined, or cancelled)

## [0.8.4] - 2026-03-09

### Fixed

- **Multi-VEVENT CalDAV sync** — recurring events with modified instances (RECURRENCE-ID) are now split and stored as separate rows during sync, so modified occurrences correctly block or free availability
  - BlueMind bundles the parent VEVENT (with RRULE) and modified instances in a single CalDAV resource; the sync now splits them using `split_vevents()`
  - New unique index `(uid, COALESCE(recurrence_id, ''))` allows parent and modified instances to coexist
  - Fixed both CLI sync and **web dashboard sync** (which was still using the old single-VEVENT logic)
  - Migration 009 was not registered in `db.rs` — now properly included

## [0.8.3] - 2026-03-09

### Changed

- **HTML email notifications** — all booking emails now use a clean, responsive HTML design with a plain text fallback
  - Color-coded accent bar: green (confirmed), amber (pending/approval), red (cancelled)
  - Structured detail table with event, date, time, guest/host info
  - Proper HTML escaping for all user-supplied values
  - `MultiPart::alternative` ensures clients without HTML support get the plain text version

## [0.8.1] - 2026-03-09

### Fixed

- **User-scoped availability on public pages** — public booking pages (`/u/{username}/{slug}` and legacy `/:slug`) incorrectly used global busy times (all users' events) instead of the host user's events only, causing other users' calendar events to block the host's available slots
- **Group slot fallback** — the group event type slot page fallback also used global busy times; now correctly scoped to the event type owner
- **Removed dead code** — `fetch_busy_times_global()` removed since all paths now use `fetch_busy_times_for_user()`



### Added

- **Admin impersonation** — admins can impersonate any user from the admin dashboard to troubleshoot their configuration (Closes #7)
  - "Impersonate" button on each user row in the admin dashboard
  - Amber banner at the top of all pages while impersonating, showing who is being impersonated
  - "Stop impersonating" button to return to the admin's own session
  - Cookie-based implementation (`calrs_impersonate`), 24-hour expiry, HttpOnly + Secure
  - Admin pages remain accessible while impersonating (uses real session, not impersonated user)
  - Dashboard shows the impersonated user's event types, bookings, and calendar sources

## [0.7.2] - 2026-03-09

### Changed

- **Internal refactoring** — extracted shared busy-time helpers (`fetch_busy_times_global`, `fetch_busy_times_for_user`, `has_conflict`, `BusySource`) eliminating ~300 lines of duplicated availability queries across booking handlers, slot computation, and group scheduling
- **Unified slot computation** — `compute_slots` now serves both individual and group event types via a `BusySource` enum, replacing the separate `compute_group_slots` function
- **Extracted `prompt()` utility** — consolidated 4 duplicate CLI prompt functions into `src/utils.rs`

## [0.7.1] - 2026-03-09

### Fixed

- **RECURRENCE-ID handling** — modified instances of recurring events (e.g., a single occurrence moved to a different time) are now properly excluded from RRULE expansion, preventing phantom duplicate occurrences in availability checks
- **Daily COUNT bug** — `FREQ=DAILY` events with `COUNT` now correctly count all occurrences from the event start, not just those within the query window; previously a COUNT-limited daily event could produce more total occurrences than intended
- **Cancelled events ignored** — events with `STATUS:CANCELLED` in the CalDAV calendar are now excluded from all availability checks (previously they still blocked time slots)
- **RECURRENCE-ID stored during sync** — the `recurrence_id` field is now extracted from iCal data and stored in the events table (migration 008)

## [0.7.0] - 2026-03-09

### Added

- **RRULE expansion** — recurring calendar events now correctly block booking availability
  - Supports FREQ=DAILY, FREQ=WEEKLY (with BYDAY), FREQ=MONTHLY (with Nth weekday BYDAY like 2MO, -1FR)
  - Handles INTERVAL, UNTIL, COUNT, and EXDATE
  - Integrated across all availability checks: public slot picker, CLI slots, booking creation validation, troubleshoot page, and group member availability
- **Availability troubleshoot page** — visual timeline at `/dashboard/troubleshoot` showing why slots are available or blocked
  - Color-coded blocks: green (available), red (calendar event), orange (booking), gray (outside hours), striped (buffer/min notice)
  - Blocked slots breakdown with event names and calendar sources
  - Event type and date selector with prev/next day navigation

### Fixed

- **Recurring events with compact date format** — events stored in iCal compact format (`YYYYMMDDTHHMMSS`) were not found by queries comparing against ISO format (`YYYY-MM-DDTHH:MM:SS`) due to string comparison; now queries compare against both formats
- **Duplicate guest emails** — guest confirmation emails used `METHOD:REQUEST` in the `.ics` attachment, causing mail servers like BlueMind to send an additional calendar invitation; changed to `METHOD:PUBLISH` (Closes #6)
- **Missing availability rules message** — troubleshoot page now shows "No availability rules for this day" instead of the misleading "All times are bookable" when no rules exist for the selected weekday

## [0.6.1] - 2026-03-09

### Security

- **Login rate limiting** — 10 attempts per IP per 15-minute window, using `X-Forwarded-For` from reverse proxy
- **Secure cookie flag** — all session and OIDC cookies now include `Secure` (HTTPS-only)
- **ICS injection protection** — user-supplied values in `.ics` invites are sanitized (CR/LF stripped, special chars escaped per RFC 5545)
- **Security documentation** — new `docs/src/security.md` covering all security measures and known limitations

## [0.6.0] - 2026-03-09

### Added

- **CalDAV write-back** — confirmed bookings are automatically pushed to the host's CalDAV calendar via PUT, and deleted on cancellation via DELETE
  - New `put_event()` and `delete_event()` methods on the CalDAV client
  - Per-source "Write bookings to" calendar selector on the dashboard
  - Bookings track which calendar they were pushed to (`caldav_calendar_href`) for accurate deletion
  - Works for individual bookings, group round-robin bookings, and pending-then-confirmed bookings
  - No configuration needed if you don't want write-back — skipped silently when no write calendar is set

## [0.5.1] - 2026-03-09

### Added

- **Dockerfile** — multi-stage build (rust:bookworm builder, debian:bookworm-slim runtime), runs as unprivileged `calrs` user
- **`.dockerignore`** — keeps build context clean
- **systemd service file** (`calrs.service`) — production-ready unit with `ProtectSystem=strict`, `NoNewPrivileges`, and other hardening directives
- **Install section in README** — Docker, Docker Compose, binary + systemd, and from-source instructions

## [0.5.0] - 2026-03-09

### Added

- **Calendar source management from the web dashboard** — add, test, sync, and remove CalDAV sources without the CLI
  - Provider selector with presets: BlueMind, Nextcloud, Fastmail, iCloud, Google, Zimbra, SOGo, Radicale
  - Auto-fills CalDAV URL and display name when selecting a provider
  - Contextual help per provider (app passwords, skip-test tips, URL patterns)
  - Connection test before saving (with "skip test" option for tricky servers)
  - One-click sync from the dashboard (full CalDAV discovery + event fetch)
  - Connection test button to verify credentials
  - Remove with confirmation dialog (cascade-deletes calendars and events)
- **Dashboard "Calendar sources" card** — lists all connected sources with URL, username, last sync time, and action buttons

## [0.4.0] - 2026-03-09

### Added

- **Group event types** — create event types owned by a group (synced from Keycloak)
  - Combined availability: slot picker shows times where any group member is free
  - Round-robin assignment: bookings assigned to the least-busy available member
  - Public group pages at `/g/{group-slug}` and `/g/{group-slug}/{slug}`
  - Group selector when creating event types from the dashboard
- **Timezone support** — guest timezone picker on slot pages
  - Browser timezone auto-detected via `Intl.DateTimeFormat`
  - Times displayed and booked in the guest's selected timezone
  - Timezone preserved across navigation (week picker, booking form)
- Project logo

## [0.3.2] - 2026-03-09

### Added

- **OIDC group sync** — groups synced from Keycloak `groups` JWT claim on each SSO login
- **Groups in admin dashboard** — group names, member counts, and per-user group badges
- Leading `/` stripped from Keycloak group paths for cleaner display

## [0.3.1] - 2026-03-09

### Added

- **OIDC authentication** — OpenID Connect SSO via Keycloak (authorization code flow with PKCE, auto-discovery, user linking by email, auto-registration)
- **Admin dashboard** at `/dashboard/admin` — user management (promote/demote, enable/disable), auth settings (registration, domain restrictions), OIDC config, SMTP status
- **Event type management UI** — create/edit event types from the web dashboard with availability schedule, location, and confirmation toggle
- **Location support** — video link, phone, in-person, or custom location on event types; displayed on public pages, emails, and `.ics` invites
- **OIDC CLI configuration** — `calrs config oidc` with interactive and flag-based modes

### Fixed

- Multiple `Set-Cookie` headers in OIDC flow (using `HeaderMap::append` instead of array tuples)

## [0.3.0] - 2026-03-09

### Added

- **Local authentication** — email/password login with Argon2 hashing, server-side sessions (30-day TTL, HttpOnly cookies)
- **User roles** — admin/user with extractors (`AuthUser`, `AdminUser`)
- **User management CLI** — `calrs user create/list/promote/demote/set-password`
- **Registration controls** — `calrs config auth` to enable/disable registration and restrict by email domain
- **User-scoped URLs** — public booking pages at `/u/{username}/{slug}`, profile pages at `/u/{username}`
- **Booking cancellation** — cancel from dashboard with optional reason, email notifications with `.ics` METHOD:CANCEL
- **Pending bookings** — event types with `requires_confirmation`; host approves/declines from dashboard
- **Web dashboard** — event types, pending approvals, upcoming bookings

## [0.2.0] - 2026-03-09

### Added

- **Web booking page** — `calrs serve` starts an Axum HTTP server with a full booking flow:
  - `GET /:slug` — public page showing available time slots for an event type
  - `GET /:slug/book?date=&time=` — booking form with name, email, and notes
  - `POST /:slug/book` — submits the booking with conflict detection, min-notice validation, and email notifications
  - Confirmation page with booking summary
- **`calrs serve [--port 3000]`** — new CLI command to start the web server
- **Minijinja templates** — clean, responsive HTML templates (base, slots, book, confirmed) with no JavaScript dependencies
- **Server-side slot computation** — reuses the same availability engine as the CLI (availability rules, buffer times, busy events, confirmed bookings)

## [0.1.3] - 2026-03-09

### Added

- **Email notifications on booking** — when a booking is created, both the guest and the host receive an email with a `.ics` calendar invite attached (METHOD:REQUEST)
- **SMTP configuration** (`calrs config smtp`) — configure SMTP server, credentials, and sender identity. Stored in SQLite
- **`calrs config show`** — display current SMTP configuration
- **`calrs config smtp-test <email>`** — send a test email to verify SMTP setup
- **`smtp_config` table** — new migration table for SMTP settings (one per account)

### Notes

- If no SMTP is configured, bookings still work — emails are simply skipped
- Tested with Scaleway Transactional Email (SWG) on port 2525 with STARTTLS

## [0.1.2] - 2026-03-09

### Added

- **`calrs booking create <slug>`** — book a slot with full validation: minimum notice, availability rules, conflict detection against both calendar events and existing bookings
- **Booking conflict detection in slots** — `calrs event-type slots` now excludes times blocked by confirmed bookings (not just calendar events)
- **README: "Connecting your calendar" section** — CalDAV URL reference table for Nextcloud, BlueMind, Fastmail, iCloud, Google, Zimbra, SOGo, Radicale with examples

### Fixed

- **Availability engine date comparison** — properly parse iCal compact dates (`YYYYMMDDTHHMMSS`) and ISO dates (`YYYY-MM-DDTHH:MM:SS`) into `NaiveDateTime` for accurate conflict detection, instead of broken string comparison across formats

## [0.1.1] - 2026-03-09

### Fixed

- **CalDAV discovery** — proper two-step discovery: principal URL → calendar-home-set → calendar listing. Previously grabbed the first `<d:href>` instead of the one inside `<d:current-user-principal>`
- **Calendar filtering** — only sync actual `<cal:calendar/>` collections, skip inbox, outbox, notifications, freebusy, and task lists
- **URL resolution** — absolute paths from the server (e.g. `/dav/calendars/...`) are now resolved against the server origin, not appended to the base URL (which caused doubled paths like `/dav/dav`)
- **iCal date parsing** — extract DTSTART/DTEND from the VEVENT block only, ignoring VTIMEZONE entries that produced incorrect 1970 dates
- **Date format handling** — calendar show now handles both `YYYYMMDD` (iCal all-day) and `YYYY-MM-DDTHH:MM:SS` formats, with proper display formatting
- **XML tag parsing** — handle tags with attributes (e.g. `<aic:calendar-color symbolic-color="custom">`) and BlueMind-specific namespace prefixes (`aic:`, `cso:`)

### Added

- **`--no-test` flag** on `calrs source add` to skip the OPTIONS connection test (needed for servers like BlueMind that don't respond to OPTIONS)
- **10-second HTTP timeout** on all CalDAV requests (60s for event fetches) to prevent infinite hangs
- **calendar-home-set discovery** step in CalDAV client (`discover_calendar_home()`)

### Tested

- Successfully syncs with **BlueMind** CalDAV (4332 events, all-day and timed)

## [0.1.0] - 2026-03-09

Initial development release. CLI-only, no web interface yet.

### Added

- **Account setup** (`calrs init`) — interactive first-time configuration with name, email, and timezone
- **CalDAV source management** (`calrs source add/list/remove/test`) — connect CalDAV servers (Nextcloud, Fastmail, iCloud, etc.), test connections, hex-encoded credential storage
- **Calendar sync** (`calrs sync`) — pull events from all CalDAV sources via PROPFIND/REPORT, upsert into local SQLite
- **Calendar viewer** (`calrs calendar show`) — display synced events in a table with date range filtering
- **Event types** (`calrs event-type create/list/slots`) — define bookable meeting templates with duration, buffers, and minimum notice. Default Mon–Fri 09:00–17:00 availability rules
- **Availability engine** — compute free slots by intersecting availability rules with synced busy events
- **Booking management** (`calrs booking list/cancel`) — view and cancel bookings
- **SQLite storage** — WAL mode, foreign keys with CASCADE, indexed queries
- **CalDAV client** — minimal RFC 4791 implementation: OPTIONS check, principal discovery, calendar listing, VEVENT fetch
