# Architecture

A map of calrs for new contributors and coding agents: what the main pieces
are, which file owns what, and where a change of a given kind goes. It is a
guide to the code, not a spec; when it disagrees with the code, the code wins
and this file needs a fix.

## Overview

calrs is a single Rust binary (`src/main.rs`). The same executable is a CLI
(`calrs source add`, `calrs sync`, `calrs user create`, ...) and, with
`calrs serve`, an axum web server that renders minijinja templates. All state
lives in one SQLite database (WAL mode) under the data directory. Calendars are
read and written through pluggable providers (CalDAV, Exchange EWS). Emails go
out over SMTP with `.ics` attachments, optional SMS through a pluggable gateway.
Every user-facing string goes through Fluent (`i18n/`).

```
 guest browser
      |
      v
 public pages (web/public_pages.rs)  --->  availability (web/availability.rs)
      |                                     rules + overrides + busy times
      | POST booking form                   (synced events, bookings, resources)
      v
 booking handlers (web/booking.rs)
      |  validate, re-check slot, insert row (pending or confirmed)
      +--> calendar write-back (web/calendar_push.rs, web/writeback.rs, providers/)
      +--> emails, spawned in the background (web/notify.rs -> email/)
      +--> SMS (sms/)
      v
 token pages from those emails (web/booking_actions.rs)
   approve, decline, cancel, reschedule, claim, ICS download

 background loop, every 60 s (run_reminder_loop in web/notify.rs)
   session cleanup, pending sweep (web/pending.rs), reminders,
   write-back retries (web/writeback.rs), sync of the stalest source
   (commands/sync.rs)
```

## Where things live

### Top level of `src/`

| Path | Owns |
|---|---|
| `src/main.rs` | CLI definition (clap), `serve` startup, spawning the background loop |
| `src/db.rs` | SQLite pool setup and the migration list (`migrate()`) |
| `src/models.rs` | Domain structs (mostly documentation; queries use tuples) |
| `src/auth.rs` | Passwords, sessions, OIDC, extractors (`AuthUser`, `AdminUser`, `OptionalAuthUser`), login/register handlers |
| `src/oauth.rs` | First-party OAuth server: discovery, public client registration, PKCE/consent, token rotation/revocation, bearer validation and cleanup |
| `src/crypto.rs` | AES-256-GCM for stored credentials, secret key management |
| `src/settings.rs` | Runtime settings (base URL, private-host policy): env overrides DB |
| `src/i18n.rs` | Fluent loader, language detection, the `t()` template global, i18n guard tests |
| `src/booking_time.rs` | How booking times are stored (legacy wall clock vs UTC instant) |
| `src/rrule.rs` | RRULE expansion for recurring events |
| `src/utils.rs` | iCal helpers (`split_vevents`, `extract_vevent_field`, datetime parsing) |
| `src/resources.rs` | Shared bookable resources: feed sync, busy intervals, pick at booking time |
| `src/google_meet.rs` | Google Meet links through the Google Calendar API |
| `src/oauth2_caldav.rs` | Google OAuth2 tokens for CalDAV access |
| `src/caldav/mod.rs` | CalDAV client: discovery, fetch, sync-collection, PUT/DELETE |
| `src/ews/` | Exchange Web Services client (SOAP, autodiscover, parsing) |
| `src/providers/` | `CalendarProvider` trait (`mod.rs`), CalDAV adapter, dispatch in `src/providers/factory.rs` |
| `src/sms/` | `SmsProvider` trait, gateway adapters, phone normalisation, message bodies |
| `src/email/` | Every email calrs sends (see below) |
| `src/web/` | The web server (see below) |
| `src/commands/` | One file per CLI subcommand; `src/commands/sync.rs` is also used by the web server |
| `src/test_support.rs` | `ENV_LOCK` / `EnvGuard` for tests touching `CALRS_*` variables |

### `src/web/`

| File | Owns |
|---|---|
| `mod.rs` | `AppState`, `create_router` (every route), middleware (CSRF, CSP, embed), rate limiter, client IP, error helpers, sidebar context, booking error page helpers |
| `public_pages.rs` | Public GET pages: user and team profiles, slot pickers, booking forms, dynamic group links, legacy redirects |
| `booking.rs` | Booking form POST handlers for every URL shape, input validation, location resolution, SMS phone mode |
| `booking_actions.rs` | Token pages reached from emails: approve, decline, guest cancel, reschedule (guest and host), claim, ICS |
| `availability.rs` | Slot computation, busy times, frequency limits, user working hours, timezone helpers, horizon, troubleshoot view |
| `hosts.rs` | Who hosts a booking: host identity, collective members, write targets, reschedule and approval hosts |
| `calendar_push.rs` | Pushing and deleting booking events on host calendars and resource collections |
| `writeback.rs` | Write-back failure recording and retry (#162) |
| `pending.rs` | Pending requests: expiry deadline, sweep (expire and nudge), pending settings on event types |
| `notify.rs` | `spawn_emails` / `drain_emails`, new-booking notifications, reminders, watchers, SMS on new booking, the background loop |
| `meeting.rs` | Auto-generated meeting links (Jitsi, webhook, Google Meet) |
| `captcha.rs` | Cap captcha config and server-side verification |
| `dashboard.rs` | Dashboard overview, event types and bookings lists, host cancel/confirm, settings, avatar |
| `event_types.rs` | Event type forms (personal and team), availability overrides, form parsing helpers |
| `invites.rs` | Invite Links page, invite management, bulk invites, quick links |
| `mcp.rs` | Stateless Streamable HTTP `/mcp`, JSON-RPC lifecycle, three scoped tools; reuses invites and availability services |
| `teams.rs` | Teams listing, creation, settings, avatars |
| `sources.rs` | Calendar sources: add, edit, test, sync, remove, write-back setup, Google connect |
| `admin.rs` | Admin panel: users, auth and OIDC, theme, captcha, SMTP, SMS, meeting links, resources, impersonation |
| `branding.rs` | Theme CSS, embed pages, static assets baked into the binary |
| `format.rs` | Date and time labels for pages, emails and the dashboard |
| `tests/` | Web tests, one file per area (below) |

`src/web/tests/mod.rs` holds the shared fixtures (`setup_test_app`, `get_authed`,
`post_form`, `body_string`, `spawn_capturing_mta`, ...). Each area file starts
with `use super::*` and sees those fixtures and every item of `web`:
`security`, `auth_pages`, `branding`, `format`, `availability`, `resources`,
`hosts`, `event_types`, `invites`, `booking`, `public_pages`,
`booking_actions`, `dashboard`, `sources`, `admin`, `emails`,
`page_language`, `guards`, plus `booking_time`, `pending_hold`,
`writeback_failures` and `organizer`.

### `src/email/`

| File | Owns |
|---|---|
| `mod.rs` | Shared data (`BookingDetails`, `CancellationDetails`, `RescheduleDetails`), language helpers, `pub use` of the files below |
| `transport.rs` | `SmtpConfig` (env and DB), `Sender` (who an email comes from), `send_email` with its timeouts |
| `render.rs` | HTML and plain-text layout, ICS generation, time conversion, `EmailParts` / `send_composed` |
| `guest.rs` | Guest emails: confirmation, pending, decline, cancellation, reminder, reschedule, pick a new time, invite |
| `host.rs` | Host, member, watcher and claimant emails |
| `tests/` | `transport.rs`, `render.rs`, `messages.rs`, shared fixtures in `tests/mod.rs` |

Call sites use `crate::email::name` whatever file `name` lives in.

### Outside `src/`

| Path | Owns |
|---|---|
| `migrations/` | SQL migrations, numbered |
| `templates/` | minijinja templates (`base.html`, `dashboard_base.html` and the pages extending them) |
| `i18n/{lang}/main.ftl` | Translations, English is the source of truth |
| `assets/` | Files embedded in the binary (logo, fonts, `embed.js`, vendored intl-tel-input) |
| `docs/src/` | User documentation (mdBook) |

## Key concepts and who owns them

- **AppState and request lifecycle** (`src/web/mod.rs`). `create_router` builds
  every route over `Arc<AppState>` (pool, templates, secret key, rate limiters,
  captcha config, CSP string, theme CSS). Middleware: `csrf_cookie_middleware`
  (double-submit cookie, checked by `verify_csrf_token` in each POST handler),
  `csp_middleware` (CSP rebuilt from captcha settings, relaxed for embeds and
  booking forms). Booking POSTs are rate limited per client IP
  (`client_ip_for_rate_limit`, rightmost `X-Forwarded-For`).
- **OAuth integrations** (`src/oauth.rs`, migration `071_oauth_server.sql`).
  Clients register automatically. Authorization uses the real browser session
  (never admin impersonation), explicit translated consent and S256 PKCE. An
  opaque, short-lived cookie resumes consent after local/SSO login. Credentials
  are random, stored hashed, bound to a user/client/resource/scope grant. Access
  tokens live one hour; rotating refresh tokens share a fixed 30-day deadline;
  replay or revocation invalidates the grant. `bearer_user` validates the audience
  and enabled user each request. Cleanup joins the hourly session sweep.
  MCP has no session state or SSE tasks and adds no dependencies. Tests:
  `src/web/tests/oauth.rs` and `src/web/tests/mcp.rs`.
- **Auth extractors** (`src/auth.rs`). `AuthUser` redirects to login,
  `AdminUser` returns 403, `OptionalAuthUser` for public pages. Each resolves
  the viewer's language once.
- **i18n** (`src/i18n.rs`, `i18n/`). Host pages use the user's saved language;
  public pages follow the visitor's browser, `/u/` pages per the host's
  "public page language" setting (`user_page_lang` in
  `src/web/public_pages.rs`); guest emails use the language stored on the
  booking. Guard tests: `src/i18n.rs` (key coverage, plurals, keys used from
  Rust), `src/web/tests/guards.rs` (lang passed to host templates, no English
  on the booking error page, escaping).
- **Email sending** (`src/web/notify.rs`, `src/email/`). Handlers never wait on
  SMTP: they hand futures to `spawn_emails`, and `drain_emails` waits for them
  at shutdown. `Sender` in `src/email/transport.rs` decides the From and
  Reply-To (guest emails go out under the host's name, #221). Each email is
  written in its reader's language.
- **Calendar providers and write-back** (`src/providers/`, `src/caldav/`,
  `src/ews/`, `src/web/calendar_push.rs`, `src/web/writeback.rs`).
  `CalendarProvider` is the trait, `src/providers/factory.rs` builds one from a
  `caldav_sources` row. Confirmed bookings are pushed to the host calendar,
  verified, and failures are recorded and retried by the background loop.
  Sync lives in `src/commands/sync.rs` (`sync_if_stale` on page loads) and
  `src/commands/sync_health.rs` (failure tracking).
- **Availability** (`src/web/availability.rs`). `compute_slots` combines
  weekly rules, date overrides, minimum notice, horizon, frequency limits and
  busy intervals (synced events, bookings including pending holds, resources).
  Team event types pick a member (round robin) or require all (collective).
  The troubleshoot view explains a day's result.
- **Booking lifecycle**. Statuses: `pending`, `confirmed`, `cancelled`,
  `declined`. Creation: `src/web/booking.rs`. Pending hold and expiry:
  `src/web/pending.rs`. Approve/decline/cancel/reschedule/claim by token:
  `src/web/booking_actions.rs`; the same from the dashboard:
  `src/web/dashboard.rs`. Storage of times: `src/booking_time.rs`. Paths
  that rebuild `BookingDetails` from the row read what the guest typed
  through `booking_guest_input`, `booking_additional_attendees` and
  `booking_ics_sequence` (`src/web/calendar_push.rs`), not their own
  queries. A trigger stamps `bookings.first_confirmed_at` (migration 071),
  so an approval can tell a rescheduled booking from a new request: it texts
  and emails the guest "moved" and sends the attendees the moved invite
  instead of "added". Every path that ends a booking reads who holds its
  invite through `with_invite_holders`, so its attendees, and the guest of a
  declined or expired reschedule, get a CANCEL only when an invite went out. A
  reschedule bumps `bookings.ics_sequence` (migration 072), the SEQUENCE
  every invite, CANCEL and push carries; so does ending a confirmed booking,
  so its CANCEL outranks the copy it ends. ICS text is folded once per
  generator by `fold_ics` (`src/email/render.rs`).
- **Organizer snapshot** (`src/web/hosts.rs`, migration
  `070_booking_organizer.sql`). Multi-host bookings store the organizer they
  were created with, so a later push never changes the ICS organizer (Google
  refuses that).
- **Resources** (`src/resources.rs`, wiring in `src/web/calendar_push.rs` and
  `src/web/admin.rs`).
- **SMS** (`src/sms/`, sends from `src/web/notify.rs` and the booking and
  action handlers, admin card in `src/web/admin.rs`).
- **Captcha** (`src/web/captcha.rs`, CSP in `src/web/mod.rs`, widget in
  `templates/book.html`).
- **Migrations** (`migrations/`, `src/db.rs`). A new file must also be added
  to the list in `migrate()` in `src/db.rs`, or it never runs on existing
  installs.
- **Background loop** (`run_reminder_loop` in `src/web/notify.rs`, spawned by
  `src/main.rs`): session cleanup, pending sweep, reminders, write-back
  retries, sync of the stalest source.

## Where to add things

- **A route or page.** Add the handler to the file that owns the area (table
  above), register it in `create_router` in `src/web/mod.rs`, add the template
  under `templates/`. Host pages pass `lang => auth_user.lang` and
  `sidebar => sidebar_context(...)`. Every POST handler checks CSRF. Tests go
  in the matching `src/web/tests/<area>.rs`.
- **An email.** Compose and send it in `src/email/guest.rs` or
  `src/email/host.rs` (guest-facing senders use `Sender::Host`; a guard test
  in `src/email/tests/transport.rs` enforces it). Call it from a handler
  through `spawn_emails`. Tests in `src/email/tests/messages.rs`.
- **A translatable string.** Add the key to `i18n/en/main.ftl` and every
  other locale, on the `i18n` branch. Use `{{ t("key") }}` in templates or
  `crate::i18n::translate` in Rust. Run `cargo test i18n::`.
- **A migration.** `migrations/NNN_description.sql` plus its entry in
  `migrate()` in `src/db.rs`.
- **A calendar provider.** Implement `CalendarProvider` (`src/providers/mod.rs`)
  and add it to `build_provider` in `src/providers/factory.rs`.
- **An SMS gateway.** One adapter file in `src/sms/` implementing
  `SmsProvider`, and one `ProviderSpec` entry in `src/sms/factory.rs`.
- **A CLI subcommand.** `src/commands/<name>.rs`, `pub mod` in
  `src/commands/mod.rs`, a variant in the `Commands` enum and a match arm in
  `src/main.rs`.

Tests sit next to their area: `#[cfg(test)] mod tests` at the bottom of most
files, `src/web/tests/` and `src/email/tests/` for the two large modules.
Tests that set `CALRS_*` environment variables take
`crate::test_support::ENV_LOCK`.
