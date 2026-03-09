# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

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
