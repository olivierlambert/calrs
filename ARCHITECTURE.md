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

 webhook task (run_webhook_loop in web/webhooks.rs)
   delivers queued booking webhooks when woken and every 30 s
```

## Where things live

### Top level of `src/`

| Path | Owns |
|---|---|
| `src/main.rs` | CLI definition (clap), `serve` startup, spawning the background loop |
| `src/db.rs` | SQLite pool setup and the migration list (`migrate()`) |
| `src/models.rs` | Domain structs (mostly documentation; queries use tuples) |
| `src/auth.rs` | Passwords, sessions, OIDC, extractors (`AuthUser`, `AdminUser`, `OptionalAuthUser`), login/register handlers |
| `src/mfa.rs` | Local TOTP two-factor authentication: enrollment, login challenges, recovery codes, the admin policy, `/auth/mfa` and `/dashboard/settings/mfa` routes (tests in `src/mfa/tests.rs`) |
| `src/oauth.rs` | First-party OAuth server: discovery, public client registration, PKCE/consent, token rotation/revocation, bearer validation and cleanup |
| `src/crypto.rs` | AES-256-GCM for stored credentials, secret key management |
| `src/settings.rs` | Runtime settings (base URL, private-host policy): env overrides DB |
| `src/i18n.rs` | Fluent loader, language detection, the `t()` template global, i18n guard tests |
| `src/booking_time.rs` | How booking times are stored (legacy wall clock vs UTC instant) |
| `src/rrule.rs` | RRULE expansion for recurring events |
| `src/utils.rs` | iCal helpers (`split_vevents`, `parse_content_line`, `extract_vevent_field`, `extract_vevent_tzid`, datetime and timezone parsing) |
| `src/tzid.rs` | Resolving an event's TZID to an IANA zone: by name (IANA, Windows, vendor prefix, display name), else by matching the resource's `VTIMEZONE`, else a display name's fixed offset (`event_timezone` at sync, `resolve_or_offset` when reading stored events) |
| `src/windows_zones.rs` | Windows timezone names (`Eastern Standard Time`) mapped to IANA ids, from CLDR's `windowsZones.xml` |
| `src/resources.rs` | Shared bookable resources: feed sync, busy intervals, pick at booking time |
| `src/booking_window.rs` | `BookingWindow`: booking horizon ∩ bookable date range, shared by web and CLI |
| `src/time_off.rs` | Per-user time off (#46): parsing a typed period, storage, and the busy intervals the slot engine adds for a user (tests in `src/time_off/tests.rs`) |
| `src/google_meet.rs` | Google Meet links through the Google Calendar API |
| `src/google_calendar.rs` | Google Calendar API beside CalDAV: out-of-office events as busy time (#240), and booking writes to Google that Google does not announce (#237: attendees set with `sendUpdates=none`, called from `CaldavClient::put_event`/`delete_event`) |
| `src/oauth2_caldav.rs` | Google OAuth2 tokens for CalDAV access |
| `src/caldav/mod.rs` | CalDAV client: discovery, fetch, sync-collection, PUT/DELETE (on Google, PUT/DELETE detour through `google_calendar`) |
| `src/caldav/multistatus.rs` | Reading WebDAV multistatus answers by namespace: calendar listing, events, sync deltas, principal and home discovery (tests in `src/caldav/multistatus/tests.rs`) |
| `src/xml.rs` | Bounded, namespace-aware XML tree over `quick-xml`, used by the CalDAV client |
| `src/ews/` | Exchange Web Services client (SOAP, autodiscover, parsing); `global.rs` holds the admin's global impersonation config, provisioning of managed sources, and the client for a managed source |
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
| `booking.rs` | Booking form POST handlers for every URL shape, input validation, location resolution, SMS phone mode. The personal and team handlers are thin: `book_personal` and `book_team` do the booking (shared with the REST API) and return a `BookingRefusal` the handler renders as before |
| `booking_actions.rs` | Token pages reached from emails: approve, decline, guest cancel, reschedule (guest and host), claim, ICS |
| `availability.rs` | Slot computation, busy times, frequency limits, user working hours, timezone helpers, horizon, troubleshoot view |
| `hosts.rs` | Who hosts a booking: host identity, collective members, write targets, reschedule and approval hosts |
| `calendar_push.rs` | Pushing and deleting booking events on host calendars and resource collections |
| `writeback.rs` | Write-back failure recording and retry (#162) |
| `write_targets.rs` | Write destination per event type (#47): which of a host's calendars a booking is written to, moving and deleting the copies written there, the choices offered in the event type form and on the sources page |
| `pending.rs` | Pending requests: expiry deadline, sweep (expire and nudge), pending settings on event types |
| `notify.rs` | `spawn_emails` / `drain_emails`, new-booking notifications, reminders, watchers, SMS on new booking, the background loop |
| `meeting.rs` | Auto-generated meeting links (Jitsi, webhook, Google Meet) |
| `webhooks.rs` | Outgoing booking webhooks (#168): `enqueue` (called by every path that changes a booking, after it commits), payload, URL checks (SSRF), signed delivery with retries (`run_webhook_loop`, spawned by `src/main.rs`) |
| `webhook_pages.rs` | The webhooks page (`/dashboard/webhooks`): add, edit, test, switch off, remove; who may manage which endpoint; the admin switch handler |
| `captcha.rs` | Cap captcha config and server-side verification |
| `ews_admin.rs` | Global Exchange impersonation: admin card, its save, confirm-address and remove-all handlers, the lock on user sources, guards on managed sources |
| `dashboard.rs` | Dashboard overview, event types and bookings lists, host cancel/confirm, settings, avatar |
| `booking_notes.rs` | Host notes on a booking (#173): who may read and edit them (`note_booking`, the one gate), the notes page and its POST |
| `overview_stats.rs` | Dashboard overview data: masthead sentence, the day ruler (bookable hours from event type rules and overrides, calrs bookings, anonymous calendar busy time), the week list, the 12-week chart, the 30-day figures (tests in `tests/overview_stats.rs`). The dashboard identity (tokens, timetable rows, status marks) lives in `templates/dashboard_base.html`; `tests/guards.rs` fails if its tells come back |
| `directory.rs` | Searchable booking directory, visibility checks, personal meeting favorites and dashboard shortcuts |
| `event_types.rs` | Event type forms (personal and team), availability overrides, form parsing helpers |
| `invites.rs` | Invite Links page, invite management, bulk invites, quick links |
| `connected_apps.rs` | Connected apps page (`/dashboard/settings/apps`): the user's OAuth grants with revoke, and the admin's revoke-all for a user |
| `mcp.rs` | Stateless Streamable HTTP `/mcp`, JSON-RPC lifecycle, three scoped tools; reuses invites and availability services |
| `api.rs` | REST API v1 under `/api/v1` (#169): Bearer authentication (OAuth tokens for `<base>/api/v1`, personal access tokens), scopes, JSON errors, rate limits, idempotency keys; event types, slots (`compute_slots`), bookings; writes call `book_personal`/`book_team` (`booking.rs`), `host_cancel_booking` (`dashboard.rs`) and `host_request_reschedule` (`booking_actions.rs`) |
| `api_tokens.rs` | API tokens page (`/dashboard/settings/api-tokens`): create (shown once), list, revoke personal access tokens; the admin switch letting every user create them |
| `teams.rs` | Teams listing, creation, settings, avatars |
| `sources.rs` | Calendar sources: add, edit, test, sync, remove, write-back setup, calendars to sync, Google connect |
| `admin.rs` | Admin panel: users, auth and OIDC, theme, captcha, SMTP, SMS, meeting links, resources, impersonation |
| `admin_stats.rs` | Admin overview figures: users, bookings per week, rankings, calendar and write-back health, inventory (aggregate queries, tests in `tests/admin_stats.rs`). `users.last_login_at` is stamped by a trigger on `sessions` (migration 080) |
| `time_off.rs` | The Time off page (`/dashboard/time-off`): list, add, delete the signed-in user's own periods |
| `branding.rs` | Theme CSS, embed pages, static assets baked into the binary |
| `listing.rs` | Shared pieces of long lists: case-blind `GLOB` search patterns bound into SQL (`ci_glob`, `ci_contains`) and the page links (`Pager`, `partials/pager.html`) used by the directory and the admin Users tab |
| `format.rs` | Date and time labels for pages, emails and the dashboard |
| `week_start.rs` | Calendar display preference: event override → account owner's default → site default (Monday); named weekday keys, independent of availability's `0=Sunday` storage |
| `tests/` | Web tests, one file per area (below) |

`src/web/tests/mod.rs` holds the shared fixtures (`setup_test_app`, `get_authed`,
`post_form`, `body_string`, `spawn_capturing_mta`, ...). Each area file starts
with `use super::*` and sees those fixtures and every item of `web`:
`security`, `auth_pages`, `branding`, `format`, `availability`, `resources`,
`hosts`, `event_types`, `invites`, `booking`, `public_pages`,
`booking_actions`, `booking_notes`, `dashboard`, `sources`, `admin`, `emails`,
`ews_global`, `page_language`, `guards`, `api`, `oauth`, `mcp`, plus `booking_time`, `pending_hold`,
`writeback_failures`, `write_destinations`, `organizer`, `duplicate_event_type`, `time_off` and `webhooks`.

### `src/email/`

| File | Owns |
|---|---|
| `mod.rs` | Shared data (`BookingDetails`, `CancellationDetails`, `RescheduleDetails`), language helpers, `pub use` of the files below |
| `transport.rs` | `SmtpConfig` (env and DB), `Sender` (who an email comes from), `send_email` with its timeouts |
| `render.rs` | HTML and plain-text layout, ICS generation, time conversion, `EmailParts` / `send_composed` |
| `footer.rs` | The guest email footer (#223): length limit, the lookup (event type, else instance default), the markdown subset rendered to HTML and plain text, with a link scheme allowlist |
| `guest.rs` | Guest emails: confirmation, pending, decline, cancellation, reminder, reschedule, pick a new time, invite |
| `host.rs` | Host, member, watcher and claimant emails |
| `tests/` | `transport.rs`, `render.rs`, `messages.rs`, `guest_footer.rs`, shared fixtures in `tests/mod.rs` |

Call sites use `crate::email::name` whatever file `name` lives in.

### Outside `src/`

| Path | Owns |
|---|---|
| `migrations/` | SQL migrations: `001` to `081` numbered, then `YYYYMMDDHHMMSS_description.sql`, embedded by `build.rs` |
| `templates/` | minijinja templates (`base.html`, `dashboard_base.html` and the pages extending them) |
| `i18n/{lang}/main.ftl` | Translations, English is the source of truth |
| `assets/` | Files embedded in the binary (logo, fonts: Inter for guest pages, Archivo for the dashboard, `embed.js`, vendored intl-tel-input) |
| `docs/src/` | User documentation (mdBook) |

## Key concepts and who owns them

- **AppState and request lifecycle** (`src/web/mod.rs`). `create_router` builds
  every route over `Arc<AppState>` (pool, templates, secret key, rate limiters,
  captcha config, CSP string, theme CSS). Middleware: `csrf_cookie_middleware`
  (double-submit cookie, checked by `verify_csrf_token` in each POST handler),
  `csp_middleware` (CSP rebuilt from captcha settings, relaxed for embeds and
  booking forms). Booking POSTs are rate limited per client IP
  (`client_ip_for_rate_limit`, rightmost `X-Forwarded-For`).
- **OAuth integrations** (`src/oauth.rs`, migrations `20261108044050_oauth_server.sql` and `20261108044051_oauth_credential_scope.sql`).
  Clients register automatically. Authorization uses the real browser session
  (never admin impersonation), explicit translated consent and S256 PKCE. An
  opaque, short-lived cookie resumes consent after local/SSO login. Credentials
  are random, stored hashed, bound to a user/client/resource/scope grant. Access
  tokens live one hour; rotating refresh tokens share a fixed 30-day deadline;
  replay or revocation invalidates the grant. `bearer_user` validates the audience
  and enabled user each request. Grants carry the consenting session's
  `mfa_verified` and `GRANT_MFA_POLICY` applies `validate_session`'s MFA rule
  to every token use; `mfa::revoke_access` deletes a user's grants with their
  sessions. Cleanup joins the hourly session sweep.
  MCP has no session state or SSE tasks and adds no dependencies. A second
  resource, `<base>/api/v1`, has its own scopes (`oauth::API_SCOPES`) for the
  REST API (`src/web/api.rs`), which also accepts personal access tokens
  (`api_tokens` table, migration `20261108093146_api_tokens`) under the same
  MFA policy and revocation. Tests: `src/web/tests/oauth.rs`,
  `src/web/tests/mcp.rs` and `src/web/tests/api.rs`.
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
  `caldav_sources` row. Confirmed bookings are pushed to the host calendar
  (the one chosen for the event type, `src/web/write_targets.rs`, else each
  source's write calendar), verified, and failures are recorded and retried
  by the background loop.
  Sync lives in `src/commands/sync.rs` (`sync_if_stale` on page loads) and
  `src/commands/sync_health.rs` (failure tracking).
- **Availability** (`src/web/availability.rs`). `compute_slots` combines
  weekly rules, date overrides, minimum notice, horizon, frequency limits and
  busy intervals (synced events, bookings including pending holds, time off,
  resources). Per-user busy time comes from `fetch_busy_times_for_user_opts`,
  the one place time off (`src/time_off.rs`) is added.
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
- **Global Exchange impersonation** (`src/ews/global.rs`, web side in
  `src/web/ews_admin.rs`, admin guide in `docs/src/ews-impersonation.md`). A
  managed source (`caldav_sources.managed = 1`) has no credentials of its own:
  every client for an EWS source goes through `ews::global::provider_for_source`,
  which picks the service account and impersonated mailbox for managed rows
  and rechecks the user (enabled, `users.email_trusted`, current address)
  every time. Only `provision_managed_ews_source_for_user` creates or moves a
  managed row; `ACTIVE_SOURCE_SQL` keeps idle managed rows out of sync and
  write-back queries; their events are stored busy-only.
- **Migrations** (`migrations/`, `build.rs`, `src/db.rs`). `build.rs` embeds
  every `.sql` file, sorted by name, and `migrate()` applies those not yet in
  `_migrations`. That table records the file stem, so a shipped migration is
  never renamed.
- **Background loop** (`run_reminder_loop` in `src/web/notify.rs`, spawned by
  `src/main.rs`): session cleanup, pending sweep, reminders, write-back
  retries, sync of the stalest source.
- **Booking webhooks** (`src/web/webhooks.rs`, page in
  `src/web/webhook_pages.rs`). A new path that changes a booking's status or
  time calls `webhooks::enqueue` after its change commits; delivery happens
  in `run_webhook_loop`, never in the request.

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
- **A migration.** `scripts/new-migration.sh short_description` creates
  `migrations/YYYYMMDDHHMMSS_short_description.sql`. Nothing to register.
  `scripts/check-migrations.sh origin/main` (also in CI) fails if it is older
  than the newest migration on `main`.
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
