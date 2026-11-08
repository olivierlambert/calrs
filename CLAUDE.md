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
| XML parsing | `quick-xml` 0.39 | CalDAV responses are XML over WebDAV; read into a namespace-aware tree by `src/xml.rs` |
| iCal | `icalendar` crate | Parsing/generating VEVENT data |
| Time | `chrono` + `chrono-tz` | Timezone handling is a known complexity area |
| IDs | `uuid` v1 | UUID v4 for all primary keys |
| Terminal output | `colored` + `tabled` | Colored text and ASCII tables in CLI output |
| Web server | `axum` 0.8 | HTTP booking page, served from CLI |
| Templates | `minijinja` 2 | Jinja2-compatible, loaded from `templates/` dir |
| OpenAPI | `utoipa` 6 | REST API document generated from the handlers (`src/web/api_doc.rs`) |
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

**Read `ARCHITECTURE.md` first.** It maps every module under `src/` (including each file of `src/web/` and `src/email/` and their test directories) to what it owns, explains the cross-cutting concepts with the file that owns each, and lists where to add a route, an email, a string, a migration or a provider. It replaces the file tree that used to live here.

The two large modules are split by feature area, so open the file for the area you are changing rather than searching one big file:

- `src/web/mod.rs` keeps `AppState`, `create_router` (every route), middleware and shared helpers; handlers live in `public_pages.rs`, `booking.rs`, `booking_actions.rs`, `availability.rs`, `dashboard.rs`, `event_types.rs`, `invites.rs`, `teams.rs`, `sources.rs`, `admin.rs`, `ews_admin.rs`, and helpers in `hosts.rs`, `calendar_push.rs`, `pending.rs`, `notify.rs`, `branding.rs`, `format.rs`. Submodules `use super::*` and `mod.rs` glob-imports them, so an item moved between files keeps its call sites. Tests are in `src/web/tests/<area>.rs` with shared fixtures in `src/web/tests/mod.rs`.
- `src/email/mod.rs` re-exports `transport.rs`, `render.rs`, `guest.rs` and `host.rs` with `pub use`, so callers keep writing `crate::email::name`. Tests are in `src/email/tests/`.

---

## Database schema (SQLite)

Migrations are tracked via `_migrations` table and run incrementally at startup via `db::migrate()`. Files are `migrations/NNN_description.sql` up to 081, then `YYYYMMDDHHMMSS_description.sql` (#333); see "When adding a new migration" below.

Key tables:

- **`users`** — multi-user: email, name, password_hash (argon2), role (admin/user), auth_provider (local/oidc), oidc_subject, username (unique), enabled flag, title, bio, avatar_path, language (saved UI language, NULL = browser), public_page_language ('visitor' or 'host', language of the user's /u/ pages), email_trusted (calrs may open the Exchange mailbox the address names, see "Global Exchange impersonation")
- **`sessions`** — server-side sessions: token (PK), user_id, expires_at (30-day TTL)
- **`auth_config`** — singleton: registration_enabled, allowed_email_domains, OIDC settings (issuer, client_id, client_secret, auto_register), `guest_email_footer` (instance default footer for guest emails, migration `20261107192406_guest_email_footer`), `guest_footer_allow_all_users` (every user may write footers; admins always may)
- **`accounts`** — scheduling accounts linked to users via `user_id`. A new user (registration, OIDC auto-register, `calrs user create`) always gets a new account from `auth::create_user_account`, in the transaction that inserts the user; an existing account is never handed over. `accounts.email` (unique) is informative only (read by the orphan migration and `calrs config dump`); an OIDC login that changes `users.email` updates it when free, and a stale copy blocking a new user's address is rewritten to its owner's current address or to a free `account-<id>[-n]@invalid` placeholder (`unused_placeholder`). `db::migrate_orphaned_accounts` links an account with no user to the user holding its address only when that user has no account, and creates no user when an address matches one only up to case or spaces; otherwise it logs a warning and leaves it
- **`caldav_sources`** — CalDAV server connections (URL, credentials, sync state, `write_calendar_href`). `enabled` flag, `ON DELETE CASCADE`
- **`calendars`**: calendar collections discovered under a source; `is_busy=1` means its events block availability by default (an event type that selects the calendar is blocked by it either way); `sync_enabled=0` (migration 079, #324, set per source at `/dashboard/sources/{id}/calendars` or `calrs source calendars`) means never fetched, no cached events, never blocks, even when an event type selects it. Busy queries check both flags; `commands::sync::set_calendar_sync_enabled` is the one way to flip it, it bumps `sync_generation`, and a sync only stores its ctag/sync token when the generation it started with is unchanged. Turning a calendar on also nulls its source's `last_synced` so the next slot page fetches it. The orphan sweep skips a write calendar that is not synced. `read_only=1` (migration `20261108100217_calendar_read_only`, Graph `canEdit: false`) keeps it out of the write-back choices, unless already selected: then it stays selected and the sources page flags it. A write destination must be a writable calendar row of the source. A Graph calendar owned by someone else starts with `is_busy=0`; nothing changes `is_busy` afterwards. Microsoft Graph sync deletes a calendar Graph no longer lists, with its events, except the source's write calendar; EWS sync has its own path, `sync_ews_source`, which removes a folder only after two listings miss it (#362). The Graph-only snapshot rules (unreadable item fails the sync, empty snapshot purges the window, unlisted calendars go) are gated by `strict_snapshots()` in `commands/sync.rs`; EWS keeps its own path. A suspended user's Microsoft sources are skipped by every sync query (`NOT_SUSPENDED_MICROSOFT`) and their tokens are never refreshed. On a Microsoft source `last_full_sync` is stamped only by a sync in which the write calendar and every calendar that counts for availability returned a snapshot (unreadable items allowed), so NULL means not read yet: `microsoft_gap_intervals()` then makes the host busy (web busy times, `calrs event-type slots` and `calrs booking create` via `host_microsoft_gaps`, per team mode), as it does past `last_full_sync` + 729 days; the sources page, troubleshoot and approvals (`approval_hosts_awaiting_first_sync`) say so. Snapshot writes and the source's freshness stamps are conditional on the calendars' `sync_generation` in the same statement. A uid vanishing from a provider calendar reaches `cancel_orphaned_booking` only when the booking was written to that very calendar (`booking_written_here`: stored `caldav_source_id`/`caldav_calendar_href`, or a `booking_write_copies` row), and that function asks `CalendarProvider::event_exists_anywhere` (Graph: the stored calendar, then every other writable calendar the account lists, skipping ones that answer 403/404; an empty listing, one without the stored calendar, or any other error is inconclusive, and inconclusive checks lasting over a day fail the sync via `caldav_sources.orphan_check_inconclusive_since`), so an event moved to another calendar never cancels a booking. An orphan cancellation deletes the booking's other copies through `caldav_delete_booking`. There is no sweep for bookings never cached on providers, as on CalDAV. A calendar still listed keeps its row and its `sync_enabled` setting even when it is not synced
- **`events`** — cached remote events from CalDAV sync; unique on `(uid, calendar_id, COALESCE(recurrence_id, ''))`, stores `raw_ical`, `etag`, `rrule`, `all_day`, `timezone`, `recurrence_id`, `status`
- **`event_types`** — bookable meeting templates (slug unique per account, team event types included since they are stored under their creator's account, and per team via `idx_event_types_team_slug` (#313); handlers check both scopes with `slug_conflict()`, `duration_min`, `buffer_before`/`buffer_after`, `min_notice_min`, `location_type`/`location_value`, `requires_confirmation`, `visibility` (public/internal/private), `max_additional_guests`, `group_id` (legacy), `team_id` (unified teams FK), `created_by_user_id`, `reminder_minutes`, `scheduling_mode` (round_robin/collective), `default_calendar_view` (month/week/column), `first_slot_only` (boolean), `pending_blocks_slot` (pending requests block the host's time, default 1), `pending_expiry_hours` (NULL = requests never expire), `booking_horizon_days` (NULL = unlimited), `bookable_from`/`bookable_until` (YYYY-MM-DD in the event type timezone, inclusive, NULL = no bound; combined with the horizon in `booking_window.rs`), `guest_email_footer` (markdown footer, max 500 chars, added to guest emails, replaces the instance default, NULL = use the default), `notes_mode` (off/optional/required, #210))
- **`event_type_questions`** / **`booking_answers`**: booking questions per event type and each booking's answers, with a snapshot of the question as the guest read it (#210, see "Booking questions" below)
- **`availability_rules`** — weekly recurring windows per event type (day_of_week 0=Sun…6=Sat, HH:MM times)
- **`availability_overrides`** — date-specific exceptions (day off, special hours). `is_blocked` flag
- **`user_time_off`**: per-user periods with no bookings on any event type they host (#46): `user_id` (`ON DELETE CASCADE`), `start_at`/`end_at` (UTC `YYYY-MM-DDTHH:MM:SSZ`, end exclusive), `timezone` (zone it was entered in, for display only), `label` (private to the user)
- **`bookings`** — bookings with `uid` (iCal), guest info, status (confirmed/pending/cancelled/declined), `cancel_token`/`reschedule_token`/`confirm_token`, `assigned_user_id` (for group round-robin), `caldav_calendar_href` (write-back tracking), `reminder_sent_at` (tracks when reminder email was sent), `pending_expires_at` (UTC deadline after which a pending request is declined, NULL = never), `pending_since` (start of the current pending window, anchors the halfway host nudge), `pending_reminder_sent_at` (that nudge went out), `first_confirmed_at` (set by trigger the first time the booking is confirmed; additional attendees get the "added" email only then, and the moved invite on a confirmed reschedule), `ics_sequence` (SEQUENCE of the invite, bumped by each reschedule and by the cancellation of a confirmed booking, so a CANCEL outranks the copy it ends, and carried by every invite, CANCEL and push), `host_notes` / `host_notes_version` / `host_notes_updated_at` / `host_notes_updated_by` / `host_notes_updated_as` (host-only notes, #173, see "Host notes" below)
- **`smtp_config`** — SMTP server settings (host, port, credentials, sender), one per account
- **`event_type_calendars`**: junction table linking event types to specific calendars for per-event-type calendar selection. Empty = use all `is_busy=1` calendars (backward-compatible default); a selected calendar counts whatever its `is_busy`
- **`event_type_write_targets`**: per-(event type, user) write destination (#47): `calendar_id` (FK calendars, `ON DELETE CASCADE`), so removing the source drops the choice. No row = the source setting (`write_calendar_href` of each source). **`booking_write_copies`** records each copy written to a destination (`booking_uid`, `user_id`, `source_id`, `calendar_href`), so later pushes, cancels and reschedules find it where it was written; pushes under the source setting record nothing
- **`booking_invites`** — tokenized invite links for internal/private event types: `token` (unique), `event_type_id`, `guest_name`, `guest_email`, `message`, `expires_at`, `max_uses`, `used_count`, `created_by_user_id`
- **`booking_attendees`** — additional attendees per booking: `booking_id` (FK), `email`, `created_at`, `added_by_claim` (the row a watcher's claim added: staff, never gets the guest footer, even after the claimant is deleted), `claimant_user_id` (whose row it is while that user exists, `ON DELETE SET NULL`)
- **`teams`** — unified teams replacing both OIDC groups-as-scheduling-units and ad-hoc team links. Fields: `name`, `slug` (unique), `description`, `avatar_path`, `visibility` (public/private), `invite_token` (for private teams), `created_by`
- **`team_members`** — team membership: `team_id`, `user_id`, `role` (admin/member), `source` (direct/group). Source tracks whether membership comes from direct assignment or OIDC group sync
- **`team_groups`** — links teams to OIDC groups for automatic member sync: `team_id`, `group_id`
- **`event_type_member_weights`** — per-event-type round-robin priority: `event_type_id`, `user_id`, `weight` (higher = assigned first)
- **`event_type_favorites`**: personal starred meetings for the booking directory and dashboard (migration 078): `user_id`, `event_type_id` (both `ON DELETE CASCADE`), `created_at`. Visibility is rechecked on every read by `directory_page()` in `src/web/directory.rs` (`is_listed()` for one id), so a favorite never grants access. Search, filter and paging (50 per page, 12 favorites on the overview) run in SQL; text search goes through `listing::ci_glob` (bound `GLOB` patterns, each character matching its case forms in every alphabet, `%`/`_`/`*` literal; control characters dropped, at most 8 distinct words), and closed booking windows are cut in SQL except today's and yesterday's, decided per zone in Rust (#340)
- **`booking_frequency_limits`** — per-event-type booking caps: `event_type_id`, `max_bookings`, `period` (day/week/month/year)
- **`groups`** / **`user_groups`** — preserved for OIDC identity sync from Keycloak. Groups are no longer used directly for scheduling; teams reference groups via `team_groups` for automatic member sync. `user_groups.weight` for round-robin priority
- **`webhooks`** / **`webhook_deliveries`**: outgoing booking webhooks (#168). `webhooks`: `user_id` (who added it; `ON DELETE SET NULL`, and a trigger deletes the user's personal-scope webhooks first), `event_type_id` (NULL = all of that user's personal event types), `url`, `secret_enc` (AES-256-GCM), `events` (comma-separated names, NULL = all), `enabled`, last attempt/status/error/success. `webhook_deliveries`: the queue, `seq` (order), `id` (idempotency id sent with every attempt), `booking_uid`, `event`, `payload` (JSON snapshot taken at enqueue), `state` (pending/failed), `attempts`, `next_attempt_at`. Delivered rows are deleted; pending ones become failed after 24 h; failed ones are pruned after 7 days; the pending rows of a webhook that stops sending are dropped (`drop_inactive_queues`, every pass and on the admin switch). `auth_config.webhooks_allow_all_users` (default 0: admins only)
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

**XML parsing (#325):** responses are parsed with `quick-xml` into a small namespace-aware tree (`src/xml.rs`), and `src/caldav/multistatus.rs` reads the multistatus from it: principal, calendar-home-set, calendar listing, REPORT events, sync-collection deltas, and the depth:0 existence check. Elements are matched by namespace URI and local name (`Element::is`), never by prefix, so `d:`, `D:`, `a:`, a default namespace or one declared on the element itself all read the same; a name under an undeclared prefix matches on its local name (old tolerance, and many test fixtures rely on it). Exception: `calendar` (in a resourcetype) and `calendar-data` are read in any namespace (`caldav_local_name`, one warning per process), since servers write them in DAV: and no other namespace defines them. Properties come only from 2xx (or status-less) propstats, first non-empty value wins; the resource-level `<status>` (direct child of `<response>`) is the only one that marks a sync deletion (#107). Malformed XML, an empty body, or a well-formed body that is not a multistatus, is an error, never "no events" (which would let the sync store the new ctag without having read anything). The client accepts only 207, or 200 with a multistatus body, on PROPFIND and on the REPORTs the sync acts on (`multistatus_body()` in `src/caldav/mod.rs`); a non-2xx answer to the time-range REPORT still falls back to the unfiltered one, and 403/405/412 on sync-collection still read as "token invalid". Status errors name the code and reason only, never the body (an error page on a shared calendar can quote other people's events, and the message is stored). A sync-collection truncated by a 507 on the collection (RFC 6578 3.6) sets `SyncResult::truncated`: the sync stores the new token but keeps the old ctag, so the next sync continues. Bodies are read through `read_body()`, capped at `xml::MAX_INPUT_BYTES` (64 MB) as they arrive, decoded with the `Content-Type` charset, and parsed on the blocking pool (`parse_blocking`). XML bounded at 128 levels, 1M elements (names and namespaces interned, about 100 MB of tree at the cap) and 256 namespace declarations in scope (quick-xml resolves names by scanning them); reading stops when the root closes (sabre/dav installs with PHP `display_errors` append warnings after it), text before the root is an error; the DOCTYPE is skipped unread and only the predefined entities and character references (`&#..;`, `&#x..;`, `&#X..;`, NUL and invalid code points kept literal) are decoded. Server-controlled names in errors are cut to 64 characters. Hrefs are kept as written (percent-encoding and absolute URLs untouched). EWS (`src/ews/soap.rs`) still has its own string extractor.

**iCal parsing:** `split_vevents()` and `extract_vevent_field()` in `utils.rs` split multi-VEVENT CalDAV blobs (e.g. BlueMind recurring events with modified instances) into individual VEVENT blocks and extract fields. Used by both CLI sync and web sync. Dates are stored as-is from iCal: `YYYYMMDD` for all-day events, `YYYYMMDDTHHMMSS` for timed events.

**Multi-VEVENT sync:** CalDAV resources may contain multiple VEVENTs (parent with RRULE + modified instances with RECURRENCE-ID). The sync splits them and stores each as a separate row with a composite unique key `(uid, COALESCE(recurrence_id, ''))`.

---

## Authentication & authorization

File: `src/auth.rs`

**Local auth:** Argon2 password hashing. Server-side sessions stored in SQLite with 30-day TTL. HttpOnly cookies (`calrs_session`).

**Local MFA (#213):** TOTP two-factor authentication for local accounts, in `src/mfa.rs` (migration 075: `user_mfa`, `mfa_recovery_codes`, `mfa_challenges`, `mfa_attempts`, `auth_config.mfa_required`, `sessions.mfa_verified`). A local login with MFA enrolled, or with MFA required, gets a short-lived challenge instead of a session (`mfa::begin_login`); no session is issued until the code is verified. Every local login and registration goes through `begin_login`; only the OIDC callback calls `create_session` directly, since SSO accounts use the identity provider's MFA. When the admin requires MFA, `validate_session` rejects local sessions that have not passed it. Wrong codes are throttled per account (`reserve_attempt`): ten in a row are free, then one try per wait, starting at 15 minutes and doubling every further ten failures up to a day; a success clears the count, recovery codes are never throttled (128 bits, and the owner's way in while a password holder burns the budget), and every refusal is logged with the user and IP. Suspending a user (`auth::set_user_enabled`), `calrs user set-password`, and linking an account to OIDC revoke its sessions, pending challenges and OAuth grants (MCP clients, #273) in the same transaction (`mfa::revoke_access`); OAuth grants also apply the same MFA policy as `validate_session` on every token use (`GRANT_MFA_POLICY` in `src/oauth.rs`); automatic OIDC linking by email is refused for an account with local MFA. Operator recovery: `calrs user reset-mfa <email>`.

**OIDC:** OpenID Connect via `openidconnect` 4.x crate. Authorization code flow with PKCE (S256). State, nonce, and PKCE verifier stored in short-lived cookies during the flow. Tested with Keycloak.

**User linking:** On OIDC callback, tries: (1) match by `oidc_subject`, (2) match by email (links existing local user), (3) auto-register if enabled. On login, `groups` and `title` JWT claims are extracted via `extract_claims_from_id_token()` and synced to the user record.

**Extractors:** `AuthUser` (redirects to login if not authenticated), `AdminUser` (returns 403 if not admin). Both implemented as axum `FromRequestParts`.

**Login/register redirect:** If the user is already authenticated, visiting `/auth/login` or `/auth/register` redirects to `/dashboard` instead of showing the form.

**URL scheme:** User-scoped public booking URLs: `/u/{username}/{slug}`. Legacy single-user routes (`/{slug}`) kept for backward compatibility.

---

## Web UI

Files: `src/web/` (see `ARCHITECTURE.md` for which file owns which page), templates in `templates/`

**Sidebar layout** (`dashboard_base.html`): All authenticated pages use a two-column layout with a persistent left sidebar (260px). Sidebar shows user avatar (with initials fallback), name, title, and organized nav sections. Mobile responsive with hamburger menu. All dashboard sub-pages pass `sidebar => sidebar_context(&auth_user, "active-page")` to their template context.

**Dashboard** — split into focused pages, each extending `dashboard_base.html`:
- `/dashboard`: overview, a greeting with one sentence about the day, the next meeting as the one large block (guest face, countdown in words, Join), a quiet "N things to set up" line, pending requests, a 7-day week strip, a 30-day sentence with the 12-week bars. Guest faces are initials on a warm hue from the name (`initials()`/`face_hue()` in `overview_stats.rs`). Faces mix the hue into the theme's `--surface`, so every preset reads. The default palette in `base.html` is warm (paper, greys, a warm charcoal in dark mode); presets and custom themes replace it as before
- `/dashboard/event-types` — Personal + team event types (create/edit/toggle/delete/view)
- `/dashboard/bookings` — Pending approval + upcoming bookings (cancel with optional reason), "Last 30 days" (meetings held) and "Cancelled and declined" (future ones and the last 30 days); both are scoped by `NOTE_HOSTS` (`booking_notes.rs`) and their titles open the host notes page (`hosted_bookings()` in `dashboard.rs`)
- `/dashboard/sources` — Calendar sources (add/test/sync/remove/write-back)
- `/dashboard/teams` — Teams listing (create/edit/manage members/delete)
- `/dashboard/invite-links` — Internal event types (personal + team) visible to authenticated users, with quick invite link generation (renamed from `/dashboard/organization`)

**Admin panel** (`/dashboard/admin`): User management (promote/demote, enable/disable), auth settings (registration toggle, allowed domains), OIDC config, SMTP status, groups overview, impersonation. Requires `AdminUser`. Past `USERS_INLINE_MAX` (100) users the Users tab is paged on the server (`UserListState` in `admin.rs`, params `uq`/`uf`/`us`/`ud`/`up`, 25 rows); below it every user is rendered and the browser filters, as before. User actions post a `back` field that is parsed and rebuilt, never echoed. The overview's booking-wide figures are cached for 60 s in `AppState.admin_figures` (`admin_stats::FiguresCache`) only on instances with more than 20,000 bookings (`FIGURES_CACHE_MIN_BOOKINGS`), so small instances stay live; the expiring-requests count behind the attention list is always live (#343).

**Public pages:** User profile (`/u/{username}`), team profile (`/team/{slug}`), time slot picker (Cal.com-style 3-panel layout with switchable month/week/column views), booking form (with optional additional attendees), confirmation page. Event types support location (video link, phone, in-person, custom). Dark/light theme toggle on all public pages. Legacy `/g/{group-slug}` URLs redirect to `/team/{slug}`.

**Theme toggle:** Class-based dark mode (`html.dark`) with inline `<head>` script for flash-free loading from `localStorage`. Public pages have a sun/moon toggle in the footer. Dashboard users can set System/Light/Dark in Profile & Settings.

**Availability troubleshoot** (`/dashboard/troubleshoot/{event_type_id}`): Visual timeline showing why slots are available or blocked, with event details. Helps debug availability issues.

**Per-event-type calendar selection:** Event type form includes a checkbox for every synced calendar. Selected calendars are stored in `event_type_calendars` junction table. When computing busy times, if no calendars are selected all `is_busy=1` calendars are checked (backward-compatible); a selected calendar blocks whatever its `is_busy`. Only synced calendars (`sync_enabled=1`) are listed or counted; a selection none of whose calendars is synced falls back to every busy synced calendar. The filter is `c.sync_enabled = 1 AND ((c.is_busy = 1 AND NOT EXISTS <synced selection>) OR c.id IN <selection>)` on `event_type_calendars` and is applied in `fetch_busy_times_for_user()`, troubleshoot handler, and CLI commands.

**Availability overrides:** Per-event-type date overrides at `/dashboard/event-types/{slug}/overrides`. Two types: blocked days (entire day off) and custom hours (replace weekly rules with specific time windows). Overrides are checked in `compute_slots_from_rules()` — blocked overrides skip the day, custom hours replace weekly rules. Also wired into CLI slot computation and troubleshoot view. Stored in `availability_overrides` table.

**Time off (#46):** per-user periods at `/dashboard/time-off` (`src/web/time_off.rs`, core in `src/time_off.rs`) and `calrs user time-off`. The single choke point is `fetch_busy_times_for_user_opts`, which appends `time_off::busy_intervals`, so personal slot pages, round robin (`pick_group_member`, the member is skipped), collective, submit-time re-checks, reschedule and `approval_hosts_free` all see it; the troubleshoot view (`time_off` status, the user's note shown since the view is only ever their own), the overview open hours (`overview_stats::without_time_off`) and the CLI `event-type slots` / `booking create` (`host_time_off`) read it directly. Every query is scoped by `user_id`; guests and teammates never see the note or the reason, and an impersonating admin gets the page read-only (add/delete refused with 403 and logged) with notes hidden, there and in troubleshoot. `time_off::add` enforces the 100-upcoming cap in its INSERT; `parse_period` refuses years past 9999 and starts over five years out; day boundaries are compared as instants (`starts_on_day_boundary`), for zones that change clocks at midnight. A failed read fails closed: slot grids get the whole window as busy (`busy_intervals`), the submit-time booking, reschedule and approval checks and the CLI get an error (`try_busy_intervals`, through the strict busy readers); `local_range` takes the hull of the local times a period covers, so its ends never reverse across a fall-back and only move when they lie inside the repeated hour; a skipped wall-clock time resolves to the first minute after the gap. The CLI checks every eligible collective member's time off. Dynamic group bookings store their co-hosts in `booking_participants` (migration `20261108005717`), and `RescheduleHosts.participants` makes the reschedule grid and the submit check require them free within their working hours and on all their calendars, and `approval_hosts_free` on all their calendars (an approval re-applies nobody's working hours); `user_busy_bookings` counts a co-host busy during their dynamic group bookings. Adding a period cancels no booking. The hourly loop prunes periods ended over 30 days ago.

**Team event types:** Created under a team from the dashboard. Two scheduling modes: round-robin (picks the least-busy available member, with configurable per-member weights) and collective (requires ALL members to be free, with per-event-type member exclusions supported: a member with weight 0 is off the roster and is not made busy by the event type's unassigned bookings; the owner is busy for them only while on the roster, #375; the overview's `HOST_SCOPE` follows the same rule). Public URLs: `/team/{slug}/{event-slug}`. Teams can be public (listed on team profile page) or private (accessible only to members). Team admins can manage members, link OIDC groups, and configure team settings at `/dashboard/teams/{id}/settings`. That page also lists the members' upcoming time off (#46), read-only, in the viewer's timezone and never with the period's label (`time_off::upcoming_for_team`, which does not select it).

**Booking watchers:** Teams can be designated as watchers on a team event type via `event_type_watchers`. On a new booking (or the approval of a new request), watchers are emailed with a tokenized "Claim this booking" link; approving a guest reschedule of a booking confirmed before does not offer it again. Tokens live in `booking_claim_tokens` with an expiry. The first watcher to claim wins; subsequent claims land on `booking_already_claimed.html`. Claimed bookings surface on the watcher's dashboard via `bookings.claimed_by_user_id`. A confirmed claimed booking is busy time for the claimant (#374): `booking_time::user_busy_bookings` is `USER_HOSTS_BOOKING` (who hosts a booking) plus the claim branch, so the claim reaches every grid, submit-time check, reschedule and approval re-check; the troubleshoot view labels a booking the user only claimed. A claim survives a reschedule, so moving or approving a claimed booking also needs the claimant free: `RescheduleHosts.claimant` (set when the claimant is not a host) joins `reschedule_busy_source` and `approval_hosts_free`, all their calendars, the booking's own row and copy excluded, no working hours.

**Event type visibility:** Three levels controlled by `visibility` column (TEXT: 'public'/'internal'/'private', migration 026). Public event types are listed on profile/group pages. Internal and private are hidden — both use tokenized invite links via `booking_invites`. Internal is available for both personal and team event types. The difference: internal event types allow **any authenticated user** to generate invite links (via the Invite Links page at `/dashboard/invite-links`), while private event types restrict invite creation to the owner. Quick link generation at `POST /dashboard/invites/{id}/quick-link` creates a single-use invite (expires 7 days) and returns JSON with the URL — available both on the Invite Links page and the per-event-type invite management page. The invite token is propagated through the booking flow via query params (`?invite=TOKEN`) and hidden form fields. Guest name/email are pre-filled from the invite (empty for quick links — guest fills them in). Token validation checks expiration and usage limits at every step. The use itself is spent by `consume_invite` (`src/web/booking.rs`), one guarded `UPDATE` inside the booking's transaction that only matches while a use is left and the invite is unexpired (#373), so concurrent submissions cannot overspend it and a failed booking spends nothing. Invite management at `/dashboard/invites/{event_type_id}` includes a "Get link" button at the top for one-click link generation, plus an email form below for sending personalized invites. Invite emails use indigo accent (#6366f1).

**Booking questions (#210):** per event type, edited in the "Booking form" section of the event type form by whoever may edit it (one hidden `questions` JSON field, validated whole by `questions::parse_editor`; posted ids are never trusted, a stored question keeps its key and kind). Five kinds: short text, long text, single choice, multiple choice, checkbox (`consent` flag, optional policy `link_url`). Limits: 20 questions, 30 options of 200 chars, label 200 (1000 for a checkbox), help 500, answers 500/5000. Tables `event_type_questions` (options as JSON with stable ids, `in_calendar`) and `booking_answers` (snapshot of label, value, option ids, consent, link, `in_calendar`, `answered_at`; `question_id` SET NULL on delete), plus `event_types.notes_mode` (off/optional/required). Everything goes through `src/web/questions.rs`: the four booking POST handlers call `read_submission` once (fields `q_<id>` and `q_<id>__<option id>`, read through `BookForm.fields`, a `#[serde(flatten)]` map) and `insert_answers` inside the booking transaction; `booking_guest_input` returns the answers so every rebuild of `BookingDetails` carries them, and a guard fails on a `BookingDetails`/`RescheduleDetails` literal that does not decide on `answers`. Shown on the bookings page and the booking notes page, in the host emails that show notes (new, approval request and reminder, confirmed, host reminder, reschedule request), in the guest's confirmation page and confirmation/pending emails, in webhooks and the REST API's booking JSON (API-created bookings, made by a manager, ask no questions: `GuestRequest.fields` is `None`), and in the host's calendar copy only for `in_calendar` questions (`generate_ics_caldav`, never the emailed `.ics`). Never in subjects, attendee copies, resource calendars, SMS or logs; the module logs nothing. Docs: `docs/src/booking-questions.md`.

**On-demand sync:** Slot pages (`/u/`, `/g/`, legacy `/{slug}`) and the troubleshoot view automatically sync the host's CalDAV sources if stale (>5 minutes since last sync). Uses `sync_if_stale()` from `commands/sync.rs` which calls `fetch_events_since()` with a time-range filter (RFC 4791) to only pull future events, with fallback to full fetch for servers that don't support it. Separately, `run_reminder_loop` syncs the stalest enabled source every 60 seconds (`next_background_source()`), which also runs each source's daily full resync; `CALRS_SYNC_MIN_AGE` (seconds, default 0, capped at a day, read once at startup) makes it skip sources whose last attempt is younger than that (#310), except a healthy source due its daily full resync, which on-demand syncs never run. With 0 the query ignores timestamps entirely, as before.

**Timezone support:** Guest timezone picker on slot pages. Browser timezone auto-detected via `Intl.DateTimeFormat`. Times displayed and booked in the guest's selected timezone. Every timezone `<select>` (slot pages, reschedule picker, Profile & Settings, event type form) is built by `timezone_options()` in `src/web/availability.rs` (#265): the common zones first, then every IANA zone from `chrono_tz::TZ_VARIANTS` in one `<optgroup>` per area, labelled with today's offset. Ids outside the areas (`US/Eastern`, `Etc/GMT+3`) and old names of a listed place (`Asia/Calcutta`, `Europe/Kiev`) are left out unless they are the selected value, which is always offered; zones merged into another one but that are places of their own (Amsterdam, Montreal) stay listed. Saves keep a value posted back unchanged even when chrono-tz cannot parse it (`resolve_saved_timezone`), and an event type with no timezone of its own stays NULL, following its owner's profile, when its edit form posts back the timezone it showed (`resolve_event_type_tz`). Since the label shows today's offset, the slot page shows a note above the times when the day shown is past a clock change (computed client-side with `Intl` `shortOffset`, in all three views).

**Avatar support:** Upload via `POST /dashboard/settings/avatar` (multipart, max 2MB, image/*). Served at `GET /avatar/{user_id}` with content-type detection. Stored in `{data_dir}/avatars/{user_id}.{ext}`. Delete via `POST /dashboard/settings/avatar/delete`.

**Admin impersonation:** Admins can impersonate any user from the admin panel to troubleshoot their view. Uses a separate `calrs_impersonate` cookie.

**Email approve/decline:** Pending bookings generate a `confirm_token`. Host notification emails include Approve/Decline buttons linking to `/booking/approve/{token}` and `/booking/decline/{token}`. These are unauthenticated public endpoints. Requires `CALRS_BASE_URL` env var.

**Pending requests (#234):** a pending booking blocks its host's (or assigned member's) busy time across all event types, like a confirmed one, when its own event type has `pending_blocks_slot = 1`. Applied in `fetch_busy_times_for_user_ex` (so every slot grid, submit-time re-check and reschedule), the troubleshoot view (labelled as a pending request) and the CLI slot/booking queries. Shared resources still block on confirmed bookings only. `stamp_pending_deadline()` sets `pending_expires_at = min(now + pending_expiry_hours, release)` with `release = start - min_notice`, or the start itself when `release` has already passed (a request inside the notice window), never earlier than now (pure helper `pending_expiry_deadline()`) when a pending booking is inserted and after a guest reschedule (which also resets the nudge). `sweep_pending_requests()`, called from `run_reminder_loop` every minute, declines expired requests with a `WHERE status = 'pending'` guard (an approval that lands first wins), emails the guest in the booking language with a rebook link, texts them (`SmsEvent::Expired`), and notifies the host; it also sends one host reminder (`send_host_approval_reminder`) halfway between `pending_since` and the deadline. Declines store no reason; the expiry is logged. Both approval paths refresh the hosts' calendars first (`prepare_approval_hosts()`, bounded by `APPROVAL_SYNC_TIMEOUT`, cached events on timeout), then hold `resources::booking_lock()` from `approval_hosts_free()` through the guarded status flip (the resource re-check runs under the same guard) and refuse (`?error=host-busy` on the dashboard, `bae-body-host-busy` on the token page) when the host became busy. The re-check counts calendar events and confirmed bookings only, excluding the booking's own row and calendar copy: other pending requests do not block an approval, so two overlapping requests cannot deadlock. An empty collective roster is refused.

**Host notes (#173):** `bookings.host_notes`, edited at `/dashboard/bookings/{id}/notes` (`src/web/booking_notes.rs`), reached from the booking title on bookings page and overview rows; an "Edit note" button only on rows with a note, no button at all on agenda rows (owner dislikes noisy rows). Read and edit share one gate, `note_booking()` (`NOTE_HOSTS`): the event type owner, the round-robin assignee, members of a collective event type (`scheduling_mode = 'collective'`, not excluded with weight 0), the claimant, by current team membership; nobody else (`dashboard_not_found()`, the same page whether the booking exists or not). Max 5000 chars (counted in characters; the textarea has no `maxlength`, a JS counter counts code points); `write_host_notes` checks `host_notes_version` and `NOTE_HOSTS` inside the UPDATE, so a host who lost access cannot write; impersonated saves record the admin in `updated_by` and the host in `updated_as`. The bookings page has a "Last 30 days" section (bookings whose start fell in the last 30 days, any of confirmed/cancelled/declined, scoped by `NOTE_HOSTS`) for post-call notes. Host emails get them as a separate `host_notes` argument read per recipient with `host_notes_for_reader(pool, uid, reader_user_id)` (the recipient must pass `NOTE_HOSTS` now: a stored collective organizer who left the team is still emailed, without notes; `booking_host_identity_with_id` gives the id; a failed lookup is logged at warn and the email goes without notes), never through `BookingDetails`/`CancellationDetails`, which also feed guest emails, ICS, write-back and SMS; `guest_facing_code_never_reads_host_notes` fails if guest-facing code or templates mention the column. Not written to CalDAV (scheduling servers forward the event to attendees).

**Guest self-cancellation:** Confirmation and pending emails include a "Cancel booking" button linking to `/booking/cancel/{cancel_token}`. Guests can cancel their own bookings with an optional reason. Cancellation updates the booking status, deletes the CalDAV event, and notifies both guest and host. Emails correctly attribute who cancelled (host vs guest).

**Booking reminders:** Background task in `calrs serve` runs every 60 seconds, sends reminder emails to both guest and host before upcoming meetings. Configurable per event type via `reminder_minutes` (NULL = no reminder). Guest reminders include a cancel button. `reminder_sent_at` on bookings prevents duplicate sends. Blue accent color (#3b82f6) for reminder emails.

**Email notifications:** Booking confirmation, cancellation, pending notice, approval request (with action buttons), decline notice — all HTML emails with plain text fallback. Confirmation and cancellation include `.ics` calendar invite attachments. Location included in emails and ICS. Additional attendees get the invite once when the booking is first confirmed, the moved invite (same UID, bumped SEQUENCE) when a reschedule is confirmed, directly or at its approval, and a cancellation with a CANCEL (no reason, which one party wrote for the other) whenever a booking confirmed at least once ends: cancelled by anyone, or a reschedule awaiting approval declined or expired; nothing while a reschedule awaits approval, and nothing at all for a request never confirmed (`with_invite_holders()` in `calendar_push.rs`). That declined or expired reschedule also attaches a CANCEL to the guest's email, since their calendar still holds the confirmed time. At a reschedule's approval the guest gets "your new time is confirmed" (`send_guest_reschedule_approved`) rather than the booking confirmation, matching the "moved" SMS (#267). ICS output is folded at 75 octets by `fold_ics()`, and the line readers in `utils.rs` unfold first.

**Guest email footer (#223):** a short markdown text (bold, italic, links; one text for all languages; 500 characters at most, counted as characters) added to every email to a guest or an additional attendee, never to hosts. `event_types.guest_email_footer` replaces `auth_config.guest_email_footer`; `email::guest_footer(pool, event_type_id)` (or `guest_footer_for_booking`) picks one and is stored on `BookingDetails`/`CancellationDetails`/`RescheduleDetails.guest_footer`. `src/email/footer.rs` renders it: input is escaped, links only for `http`, `https`, `mailto`, `tel`, and the plain-text part writes `label (url)`. Guest composers pass it through `EmailParts.guest_footer` (HTML block above the "sent by calrs" bar, plain text above the signature); host composers set it `None`. Who may write it: admins, or everyone when `auth_config.guest_footer_allow_all_users` is on (`can_edit_guest_footer`, `resolve_guest_footer` in `web/booking.rs`; a user who may not edit keeps the stored value, and Duplicate copies it only for who may edit). Edited on the event type form (Access & notifications) and in the admin panel (`POST /dashboard/admin/guest-footer`). A guard test (`every_guest_email_details_literal_sets_the_guest_footer`) fails when a new construction of the details structs under `src/web` or `src/commands` neither sets `guest_footer` nor copies it, which is how a new send path would silently drop it.

**CalDAV write-back:** Confirmed bookings are pushed to the host's CalDAV calendar (if `write_calendar_href` is configured on the source). On cancellation, the event is deleted from CalDAV.

**Google writes and invitations (#237):** Google's CalDAV ignores `SCHEDULE-AGENT=CLIENT` and mails attendees itself, while calrs sends its own invitation. `CaldavClient::put_event`/`delete_event` therefore detour through `google_calendar` for a Google client (OAuth2 bearer on `apidata.googleusercontent.com`): the ICS is PUT without ATTENDEE/ORGANIZER lines (`split_scheduling`), then the attendees are set with Calendar API `events.patch?sendUpdates=none`; an existing event with attendees has them cleared the same way before a rewrite; a delete of an event with attendees uses `events.delete?sendUpdates=none`. The Meet conference and time patches carry `sendUpdates=none` too (`event_patch_url`). Guests stay attendees so Meet admits them without the lobby. Without the Calendar API the event is still written, host-only; a delete falls back to CalDAV. Callers pass the ordinary `generate_ics_caldav` ICS; do not add a Google-specific generator. Only one Google copy per booking carries attendees, since Google adds the event to each attendee's own Google Calendar: `google_calendar::attendee_copy_holder` (the `elect_meet_owner` user, the primary host when there is no Meet), on that user's Google row with the lowest source id among the rows `push_to_sources` writes (so a #47 destination follows the rule); every other Google copy, and every claimant copy, goes through `split_scheduling` without attendees. Deletes and releases of recorded copies go through `CaldavClient::delete_event`, so a copy with attendees is deleted through the Calendar API.

**Google out-of-office (#240):** `google_calendar::sync_out_of_office` reads `eventType=outOfOffice` events of the primary Google calendar through the Calendar API on every sync (90 days back, 366 ahead, `singleEvents=true`) and replaces them atomically as `events` rows with `google_out_of_office = 1` (UTC, OPAQUE, uid `google-out-of-office:{id}`; a whole-day `date` item covers its days in the calendar's `timeZone`; an unreadable item is skipped and logged, a repeated id stored once); a failed fetch keeps the previous snapshot and fails the source's sync (`refresh_out_of_office`, so `sync_health` and the owner alert apply; not when the API is just not enabled and nothing is cached), and rows not refreshed for `SNAPSHOT_MAX_AGE_DAYS` (7) are dropped. A refresh is skipped within 5 min of the last successful one (in memory) and bounded by `REFRESH_BUDGET` (10 s), since on-demand syncs run inside slot page requests. CalDAV delta deletion and the orphan sweep skip those rows. Busy queries fetch them with a two-day margin and filter after converting to the host zone. They follow `is_busy`, `sync_enabled` and per-event-type calendar selection like any synced event.

**Write destination per event type (#47):** `src/web/write_targets.rs`. `host_write_sources(booking, user)` decides where a host's copy goes: the calendar the user chose for the booking's event type (only that one), else every source's `write_calendar_href` as before. Per user, so on a team event type each member chooses their own (assigned member, each collective member). Chosen in the personal event type form (`write_calendar_id`, editor's own row) and for every hosted event type on `/dashboard/sources` (`POST /dashboard/sources/destinations`, one `et_<id>` field each); team forms only point there. `set_destination` refuses a calendar outside the user's own enabled sources and an event type they do not host, and `destination_source` re-checks ownership and `cs.enabled` at read time (disabled source = fallback; `sync_enabled = 0` is allowed, like the source write calendar, #324). `push_and_record` pushes every target first (a destination answering 403/404/405 falls back to the source setting at once, noted in `last_refused_at` for the sources page), then, for a booking written before and only for targets whose push landed, `release_moved_copies` and `release_former_hosts` delete copies no longer where the booking goes (never a collection another target wrote to), and `repoint_href` keeps `caldav_calendar_href` off a released copy: deleting first would let the orphan sweep cancel a live booking. Places are (source, href): `bookings.caldav_source_id` stores the source the href is on, so the orphan sweep and deletes never take the same path on another server for the booking's calendar. Pushes of one booking are serialised (`booking_push_lock` in `writeback.rs`). A permanent refusal is a typed `providers::PermanentRefusal` (CalDAV 403/404/405 except Google rate limits, EWS `ErrorAccessDenied`/`ErrorFolderNotFound`); with no source write calendar to fall back to it stays a write-back failure. A first move records the source-setting copies before pushing, so they are released or found by a cancellation. A released copy rewrites the claimant's copy. Delete paths (`caldav_delete_booking`, `caldav_delete_for_user`) delete recorded copies first, for current hosts and anyone else with a copy, comparing (source, href); a row is forgotten only once its DELETE succeeded (404/410 count), and `sweep_leftover_copies` in the background loop retries those of ended bookings on the `RETRY_DELAYS_MIN` schedule, then every 8 hours until the booking is over (its rows are then dropped). Removing a source clears the href and source id of bookings pointing at it (trigger `bookings_forget_removed_source`). An unverified write to a destination keeps the href on the copy known to exist. Copies on a disabled source are left alone and stay recorded. A stored choice on a disabled source stays selected as unavailable and survives saving unchanged. A Google Meet holder's copy follows `google_meet::google_write_source_for_booking`: an existing recorded copy, else (not created yet) the destination if it is a Google calendar of theirs, else the Google source write calendar; the Meet never moves. The orphan sweep also matches a stored href recorded in `booking_write_copies` on a synced calendar of that source. A Google destination also makes its host eligible as Meet owner (`elect_meet_owner`, `users_missing_google_writeback`). Claimant copies keep the source setting.

**Write-back failures (#162):** every push goes through `caldav_push_booking` → `web::writeback::push_and_record`; do not call `caldav_push_booking_for_user` directly. A fresh push also writes the claimant's copy of a claimed booking (`push_claimant_copy`, `CopyOwner::Claimant`, which never touches `caldav_calendar_href`; failures are only logged), and `caldav_delete_booking` deletes that copy by uid from the claimant's write sources (`claimant_delete_booking`, also called by the sync orphan path) before anything else, so it follows every cancel and reschedule; a pending reschedule deletes it even when the host has no href (#269). The claimant's copy is skipped when the claimant is a host or owns the booking's Google Meet event (`google_meet::holds_the_meet`), and not written to a collection a host also writes to. A fresh push that finishes on a booking no longer confirmed deletes what it wrote. Only a confirmed, unclaimed booking can be claimed (guarded UPDATE). Each write is `providers::put_and_verify` (PUT, then `CalendarProvider::event_exists` by UID); a refused or unreadable write is a failure. The booking is **never** refused or rolled back for it (slot is held by calrs's DB, guest has the invite): the failure goes to `bookings.writeback_error`/`writeback_attempts`/`writeback_next_attempt_at`, and `retry_failed_writebacks` (in `run_reminder_loop`) retries confirmed future bookings at 5/15/30/60/120/240/480 min (8 attempts total). After `ALERT_AFTER_ATTEMPTS` (3) the failing hosts get one email (`writeback_alerted_at`, claimed atomically, cleared only on success). A fresh push restarts the attempt count. `caldav_calendar_href` is stored whenever the PUT was accepted, even if the read-back failed, so cancellation still deletes. The bookings dashboard shows a badge.

**Source sync failures (#162):** `sync_source`/`sync_ews_source` record their outcome via `commands::sync_health`; `sync::sync_one_source` is the single client dispatch (background, on-demand, dashboard button, CLI) and records pre-request failures too. A failed calendar fails its source's sync. Availability keeps being served from the cache. The owner is emailed once when `sync_failure_count >= 3` and failing for over 60 min; success clears everything and re-arms. On-demand syncs skip sources that failed in the last 5 min (`last_sync_error_at`), and the background loop orders by the later of `last_synced`/`last_sync_error_at`. `calrs sync` exits 1 when any source failed.

**REST API (#169):** `src/web/api.rs`, JSON under `/api/v1`, documented in `docs/src/api.md`. Bearer only, cookies never read (no CSRF surface), no CORS, a foreign `Origin` refused, 16 KiB bodies, `{"error":{"code","message"}}` everywhere. Tokens: OAuth access tokens for the resource `<base>/api/v1` (`oauth::api_resource`, scopes `oauth::API_SCOPES`, a grant is bound to one resource) or personal access tokens (`calrs_pat_` + 43 chars, `api_tokens` table, hashed, created at `/dashboard/settings/api-tokens` by `src/web/api_tokens.rs`; admins only unless `auth_config.api_tokens_allow_all_users`, which `oauth::api_allowed` also applies to OAuth API grants at consent and at every use; MCP is not covered). Both follow `GRANT_MFA_POLICY`, and `mfa::revoke_access`, `delete_unverified_local_grants` and the admin's revoke-all delete PATs too. Scope: event types the user manages (`MANAGES_EVENT_TYPE` in `directory.rs`, no admin bypass, plus directory internal types read-only via `distributable_event_types`, shared with MCP); bookings by `NOTE_HOSTS`; cancel/request-reschedule by the dashboard's owner rule. Writes reuse `book_personal`/`book_team` (`booking.rs`, with `BookingAccess::Manager`: no invite), `host_cancel_booking` (`dashboard.rs`) and `host_request_reschedule` (`booking_actions.rs`); the web handlers call the same functions and render `BookingRefusal::into_page`. Rate limits: `api_limiter` 300/min per token, `api_heavy_limiter` 60/5 min per user for slots and POSTs. `Idempotency-Key` on `POST /bookings` (`api_idempotency`, per user, 24 h, 5xx not stored, a lease renewed while running, replays re-check access). Email caps: 50 API bookings per user per 24 h (`api_booking_log`) and 10 attendees each. Writes answer `{id, status}` unless the token also has `bookings:read`. `host_request_reschedule` refuses a second request while `reschedule_by_host = 1` (web and API). **OpenAPI:** `src/web/api_doc.rs` generates an OpenAPI 3.1 document with `utoipa` (MSRV 1.88) from `#[utoipa::path]` on every handler and `ToSchema` on the request and answer structs the handlers serialize (`EventTypeList`, `Booking`, `WriteAnswer`, `ErrorBody` with the `ErrorCode` enum every `ApiError` carries); `CommonAnswers` adds 401/403/429/500, the error envelope and headers to every authenticated operation; `document()` sets `servers` and the OAuth URLs from the base URL. Public at `GET /api/v1/openapi.json`. A new endpoint needs its `#[utoipa::path]` and an entry in `ApiDoc`'s `paths`: `tests/api_doc.rs` fails when `api::router()` and the document disagree, when an operation's security scope differs from its handler's `authenticate` scope, when an answer does not match its schema, or when an error code is described under no status. `/api/docs` (AuthUser) renders it with Swagger UI vendored in `assets/swagger-ui/` (licences and SHA-256 pinned by a test; `SWAGGER_UI_VERSION` busts the cache), under the baseline CSP; its requests omit credentials, so "Try it out" works only with a pasted personal access token, and its copy of the document drops the `oauth2` scheme (the page cannot complete that flow).

**Global Exchange impersonation (#138):** one EWS endpoint and one service account with `ApplicationImpersonation` (Access tab card `#ews-global`, `src/web/ews_admin.rs`; config, provisioning and the provider in `src/ews/global.rs`; docs in `docs/src/ews-impersonation.md`). Each user gets a managed row in `caldav_sources` (`managed = 1`, `impersonate_email` = their address lowercased after the domain override, no password). Rules: only a trusted address is provisioned (`users.email_trusted`: 1 for `calrs user create`, set from `email_verified` at OIDC account creation and at every login, 0 for self-registration and for accounts that predate it, until an admin confirms them on the card or with `calrs user trust-email`); an OIDC login that is not verified deletes the user's managed source (`after_oidc_login`). Every path goes through `provision_managed_ews_source_for_user`, which reads the address from `users`, and `provider_for_source` rechecks user, trust and mailbox before every sync or write. Registration's "email taken" check is case-insensitive. The domain override (`ews_impersonation_from_domain` to `ews_impersonation_domain`) rewrites only addresses in exactly the from-domain; an address without exactly one `@` names no mailbox; one mailbox serves one user (unique index, conflicts shown in the status table). Managed sources with nothing to do (feature off or incomplete, user suspended or untrusted) are filtered out by `ACTIVE_SOURCE_SQL` in the background, on-demand and CLI sync queries and in write-back, and are skipped without recording a failure. Events from a managed source are stored busy-only (`upsert_provider_events(.., busy_only)`): no summary, description, location or iCal text, except a bare DTSTART/EXDATE VEVENT for a recurring one. `set_write_calendar` only accepts one of the source's own calendars. The global URL must be https, and the stored service password is cleared when the URL or account changes without a new one. Addresses are ASCII-only for `mailbox_for`; an IdP-verified owner (`users.email_verified_by_idp`) takes a mailbox over from an admin-confirmed holder. `ews::global::mailbox_lock` serializes a mailbox change, and the removals in `ews::global` (one user, the untrusted, all) and the admin's per-user remove, with the commit phase of an EWS sync, which discards results fetched for a mailbox the source no longer reads. Other deletions of a source row (a user removing their own source, `calrs source remove`, deleting a user) do not take it; a sync running then finds its row gone at the commit phase and stores nothing. Calendar folders are listed page by page (`IndexedPageFolderView`); a folder is dropped after two complete listings miss it; managed calendars are stored as "Exchange calendar N". "Remove managed sources and clear the service password" (`POST /dashboard/admin/ews/remove`) deletes them all and switches the feature off; bookings already written into mailboxes stay there.

**Security hardening (1.0):**
- **CSRF protection** — double-submit cookie pattern on all 31 POST handlers via `csrf_cookie_middleware`. Client-side JS injects `_csrf` hidden field. Multipart forms use query parameter.
- **Booking rate limiting** — per-IP (10 req / 5 min) on all 4 booking handlers. Client IP is the *rightmost* `X-Forwarded-For` value (the trusted proxy's view of its peer; leftmost is attacker-controlled). See `client_ip_for_rate_limit()` in `src/web/mod.rs`.
- **Input validation** — server-side on all booking forms (name 1–255, email format, notes max 5000, date max 365 days), registration, settings, avatar upload (content-type whitelist).
- **Double-booking prevention**: partial unique index `idx_bookings_no_overlap` on `(event_type_id, start_at)`, plus `resources::booking_lock()` taken by every booking core, guest reschedule and approval from the first submit-time check through the write. The booking transaction (sqlx's deferred `BEGIN`) only wraps the writes and is opened after the checks: never wait for a pool connection, or for the lock, while holding a connection (the pool has 5). Submit-time checks use the strict readers (`try_fetch_busy_times_for_user*`, `try_user_avail_as_busy`, `try_team_busy_source`, `pick_group_member`, `reschedule_busy_source(.., true)`, `approval_hosts_free`, `reschedule_hosts`, `is_offered_slot`, `check_and_pick`, all `Result`), and a read error refuses with the internal error page; slot grids keep the lenient ones. Nothing under the lock may do network I/O (feed fetch, CalDAV sync, SMTP, DNS): feeds and calendars are refreshed before it, and the submit-time `is_offered_slot` reads resources from the cache (`try_compute_slots_cached`, `try_cached_blocking_intervals_for_event_type`). The lock is in-process: the CLI `calrs booking create` runs in its own process and relies on the database guarantees only (the overlap index, `legacy_slot_taken`). Regression tests in `src/web/tests/concurrency.rs`.
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

Configured at `/dashboard/admin` → **Captcha** section. Five fields:

| Field | Required | Description |
|---|---|---|
| Instance URL | Yes | Base URL of your Cap server, e.g. `https://captcha.example.com` |
| Site key | Yes | Site key from the Cap dashboard |
| Secret | Yes | Secret key for server-to-server verification (encrypted at rest with AES-256-GCM, same pattern as OIDC client secret) |
| Widget script URL | No | Override the JS bundle URL. Defaults to `https://cdn.jsdelivr.net/npm/cap-widget`. Useful for air-gapped deployments. Changes take effect immediately after saving (CSP is rebuilt in memory). |
| Widget WASM URL | No | Where the widget fetches its WASM solver. When set, `book.html` emits `window.CAP_CUSTOM_WASM_URL` (via `tojson`) before the widget script, so a self-hosted Cap stops pulling the WASM from cdn.jsdelivr.net (#164). Must be an absolute http(s) URL; the admin handler refuses anything else. Its origin is added to `connect-src`. |

Leaving all three main fields empty disables the captcha. The secret uses the keep-current pattern: leaving the password field empty on save preserves the stored value.

### How it works end-to-end

**GET (booking form):** The handler reads `state.captcha_config` (a `RwLock<Option<CaptchaConfig>>`). If `Some`, it passes `captcha_enabled = true`, `captcha_api_endpoint`, `captcha_widget_url` and `captcha_wasm_url` (empty when unset) to the template. The `<cap-widget>` element is rendered conditionally in `templates/book.html` with all i18n attributes pre-filled via the Fluent `t()` helper.

**POST (booking submit):** The `BookForm` struct has a `captcha_token: Option<String>` field with `#[serde(rename = "cap-token")]` — the field name the Cap widget submits. After the CSRF check, `captcha::verify(&captcha_cfg, form.captcha_token.as_deref()).await` is called:
- `config = None` → `Ok(())` immediately (pass-through)
- `config = Some`, token missing/empty → `Err(())` → renders `booking_action_error.html`
- `config = Some`, token present → POST to `<instance>/<site-key>/siteverify` with JSON `{"secret": "...", "response": "<token>"}` → parses `{"success": bool}`

The verification is implemented in `src/web/captcha.rs` and applies to **3 handlers**: `handle_booking`, `handle_booking_for_user`, `handle_group_booking`. The dynamic group booking path (`username.contains('+')`) is also protected because the check happens before the branch.

### Code structure

| File | Role |
|---|---|
| `src/web/captcha.rs` | Self-contained module: `CaptchaConfig` struct, `load_captcha_config()`, `verify()`, `extract_origin()` helper, unit tests |
| `src/web/mod.rs` | `AppState.captcha_config: RwLock<Option<CaptchaConfig>>`, `build_csp()`, `csp_middleware()` |
| `src/web/admin.rs` | `admin_update_captcha()` handler |
| `src/web/public_pages.rs`, `src/web/booking.rs` | wiring in the 4 GET form handlers and 3 POST booking handlers |
| `migrations/053_captcha.sql` | Adds `captcha_instance_url`, `captcha_site_key`, `captcha_secret`, `captcha_widget_url` columns to `auth_config` |
| `migrations/067_captcha_wasm_url.sql` | Adds `captcha_wasm_url` to `auth_config` |
| `templates/admin.html` | Captcha configuration section (status badge, 5 inputs, save button) |
| `templates/book.html` | Conditional `<cap-widget>` with all `data-cap-i18n-*` attributes, preceded by the `CAP_CUSTOM_WASM_URL` assignment when a WASM URL is set |
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
connect-src 'self' <instance-origin> <widget-origin> [<wasm-origin>];
object-src 'none'; base-uri 'self'; frame-ancestors 'self'
```

`<wasm-origin>` appears only when a Widget WASM URL is configured and its origin is not already one of the other two.

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

**Read path** (`src/resources.rs`): feeds are cached in `resource_events` and re-synced when older than 5 minutes (`sync_if_stale`; the feed is authoritative, orphans are deleted; failed fetches also stamp `last_synced_at` so dead feeds back off, and record `last_sync_error` for the admin panel; success clears it). `fetch_feed` re-validates the URL against the private-host policy on every fetch, follows no redirects, and caps body size and event count. `busy_for_resource()` merges feed events (single + RRULE-expanded, skipping CANCELLED/TRANSPARENT, excluding the booking's own reservation and reservations of cancelled bookings) with calrs' own confirmed bookings, so two event types sharing a resource cannot double-book it even without write-back. `blocking_intervals_for_event_type()` feeds `compute_slots()`; `check_and_pick()` is the booking-time gate (a read error is an `Err`, never "free"), serialized by the process-wide `booking_lock()` every booking holds from its first check until the booking row is committed. Both approval paths re-check resources (pending bookings do not block them). The troubleshoot view shows blocked intervals as `resource_busy`.

**Write path** (reservation, `src/web/calendar_push.rs`): confirmed bookings are PUT into the required resources' CalDAV collections under the booking's own uid (`resource_push_booking`; every attached resource in `all` mode, only the assigned resource in `round_robin`), and deleted when a confirmed booking is cancelled or rescheduled (`resource_delete_booking`, targets derived from the stored assignment so detached resources still get released). Declined pending bookings need no cleanup: reservations are only pushed on confirmation. The CalDAV URL is either configured or derived from a BlueMind publish URL (`derive_caldav_url`: `/api/calendars/publish/calendar:UID/...` maps to `/dav/calendars/__uids__/UID/calendar:UID/`). Credential candidates in trust order (`resource_write_candidates`): the resource's service account first, then members who opted in via `users.lend_resource_write` and have a CalDAV source on the same scheme+host+port, preferring the booking's assigned host. Note: lending grants writes to any collection on that origin the member's own server-side ACL allows; the service account is always preferred. Write failure is non-fatal: the DB-side busy check keeps blocking the resource, and failures are logged.

**Visibility:** host-facing emails (new booking, approval request, confirmed, host reminder) show a "Resource" row via `BookingDetails.resource_name` (`booking_resource_label()`: assigned resource in round_robin, attached names in 'all'); guests never see resource names. The event-types listing shows a "resources" badge, and the bookings dashboard shows the assigned resource per booking.

**Admin UI:** `/dashboard/admin` Resources card: add (feed validated and synced on create, name auto-filled from `X-WR-CALNAME`), edit (keep-current password pattern; feed re-validated and re-synced), delete, "Sync now", "Test write" (PUT/verify/DELETE cycle with a temp event 24h out). Members opt in to credential lending in Profile & Settings. The event type form gains a "Required resources" checkbox section + mode radio, visible to admins only.

**CLI:** `calrs resource probe --url <URL> [--username U] [--write-test]` probes a feed or CalDAV collection (full RFC 4791 discovery fallback, write test with a temporary event). Known gap: `calrs event-type slots` and `calrs booking create` do not consult resources yet; the web paths do.

**Files:** `src/resources.rs` (core logic + tests), `src/web/admin.rs` (admin handlers), `src/web/booking.rs` (booking wiring), `src/web/calendar_push.rs` (write-back), `src/commands/resource.rs` (probe CLI), `migrations/058_resources.sql`, `templates/admin.html`, `templates/event_type_form.html`, `templates/settings.html`, `templates/troubleshoot.html`.

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

**Phone numbers:** the guest types whatever they like (`06 12 34 56 78`, `0033612345678`, `+33 6 12 34 56 78`); `phone::normalize()` converts to E.164 server-side using the configured default country code, and `bookings.guest_phone` is E.164 from then on. This is not libphonenumber: it handles trunk and international prefixes and leaves real validity to the gateway. Numbers are shown to the host on the bookings dashboard and never to other guests. Once an event type's mode is `off`, a number stored before is hidden everywhere (emails, webhooks, calendar copies, dashboard, REST API): read it through `booking_guest_input` or filter with `sms::collects_phone`.

**Phone modes** (`event_types.sms_phone_mode`, three states rather than a boolean, same shape Cal.com converged on): `off` shows no field; `optional` shows one and says plainly that an empty answer means no SMS, so the guest is never silently dropped from a channel they expected; `required` enforces it, for event types where the message is the point. `book.html` renders the field through the vendored intl-tel-input widget (see below), so a bad number is an inline field error rather than a full-page one; `resolve_guest_phone()` is the server-side backstop and returns the localised (title, message) pair to render.

**Country picker** (`assets/intl-tel-input/`, vendored at 25.3.1): the phone field is an intl-tel-input widget with a flag picker, format-as-you-type, and libphonenumber validation. All seven files are baked into the binary and served from `/static/intl-tel-input/` by one allowlist handler, so a booking page makes no third-party request; `utils.js` is 265 KB of libphonenumber and is lazy-loaded via `loadUtils` rather than linked on every page. Two licences apply, MIT for the widget and Apache-2.0 for `utils.js`, both recorded in `assets/intl-tel-input/README.md` and pinned by a test.

The country is **seeded, not guessed**. Only an explicit BCP-47 region subtag counts (`fr-FR`, `pt-BR`); otherwise the seed is `sms_config.default_country_code` mapped to a country through the widget's own data. A bare language tag is deliberately ignored, because a language is not a country: `sv` is Swedish but reads as El Salvador, `pt` would send Brazilian guests to Portugal, and `uk` is Ukrainian rather than the United Kingdom. The dial-code lookup honours the `priority` field, without which the name-sorted list resolves `+1` to American Samoa and `+44` to Guernsey. Since the flag is visible and editable, a wrong seed is a visible default rather than a silent rewrite. The visible input keeps `name="phone"`, so with no JavaScript the field still posts and `phone::normalize()` still resolves it against the configured country.

**Spend controls.** SMS is the one feature where a public, unauthenticated form spends real money on a recipient the guest chooses, which is the SMS pumping (AIT) attack. Three layers:
- **Who may enable it**: `auth_config.sms_allow_all_users`, off by default, so only admins can put an event type into an SMS mode (same reasoning as shared resources). `can_enable_sms()` and `resolve_phone_mode()` enforce it server-side; a user who may not change it has the stored value carried forward, so a member editing an admin's event type cannot turn SMS off either.
- **How much**: `sms_config.daily_cap` (0 = unlimited) checked in `notify_guest()` against `sms_usage`, which records segments and cost per accepted message and deliberately stores no recipient number. Over the cap, calrs stops texting and email carries on: a booking must never fail because the SMS budget ran out. The admin panel shows today's count and cost.
- **Outside calrs**: the admin card tells the operator to restrict destination countries at the gateway (Twilio calls this Geo Permissions), keep the account prepaid without auto-recharge, and leave the captcha on. The panel warns when SMS is configured and the captcha is not, since that combination is the actual open relay.

**Guest ICS:** `GET /booking/ics/{cancel_token}` serves the booking as `text/calendar`, and `confirmed.html` links it as "Add to calendar". The confirmation email already attaches one, but the guest is looking at the page at the moment they want to add it, and an instance with no SMTP never sends that email at all.

---

## Booking webhooks

Files: `src/web/webhooks.rs` (queue, payload, URL checks, delivery, `run_webhook_loop` spawned by `serve`), `src/web/webhook_pages.rs` (`/dashboard/webhooks`, the admin switch at `/dashboard/admin/webhooks/policy`), `templates/webhooks.html`, docs in `docs/src/webhooks.md`.

- **Events**: `booking.created` / `booking.requested` (new booking, confirmed or pending), `booking.confirmed` (either approval path), `booking.declined` (token or dashboard), `booking.expired` (pending sweep), `booking.cancelled` (guest token, host dashboard, CLI; `cancelled_by`), `booking.rescheduled` (`guest_reschedule_booking`, the one path that moves a booking; `previous` holds the old times), `booking.claimed` (`claim_booking`; `claimed_by`), `ping` (Send test). Each payload's `status` is the event's (`Event::status`), not the row's at payload time. **Any new path that changes a booking's status or time must call `webhooks::enqueue(pool, id_or_uid, Event, Context)` right after its change commits, before slow work (calendar deletes, resource releases).** Where slow work must come first because the payload needs it (the meeting link of a new or approved booking), call `webhooks::reserve` at commit and `Reserved::fill` afterwards: the reserved rows (state `reserved`, filled in the background if dropped) hold the event's place, so a cancellation in between is ordered after it. It never fails and costs one `SELECT EXISTS` when no webhook is configured. The CLI enqueues too; the server's 30 s tick delivers.
- **Scope and rights**: an endpoint on one event type is managed by `can_manage_event_type` (owner, team admins, instance admins); an endpoint with `event_type_id` NULL covers the adder's personal event types only (never team ones) and only the adder sees it. Non-admins need `webhooks_allow_all_users`; the switch and the adder's `enabled` flag are re-read at enqueue and at delivery (`ACTIVE_ENDPOINT`), so turning it off stops queued deliveries too. A webhook only sends while its user can still manage its scope (`MANAGED`: enabled, and instance admin, owner of the personal event type, or admin of its team); otherwise it is suspended (kept, shown as such, queue dropped) until a current manager takes it over (`/dashboard/webhooks/{id}/adopt`: only a suspended event type webhook, guarded in the UPDATE; the manager confirms the URL and sets a new secret, the queue is dropped, and they become its user). Only its user sees the full URL (`masked_url` for the others). Any URL change drops the queue; a URL on another host needs a new secret. An event type's webhook survives its user's deletion that way.
- **Event selection**: one checkbox per event (`ALL_EVENTS`), stored by `events_column()` (all ticked = NULL, so later events are included). `enqueue` skips unsubscribed events. "Send test" ignores the selection and is rate limited per user (`AppState.webhook_test_limiter`, 10 per 10 min).
- **Payload** (version 1): `id`, `event`, `occurred_at`, per-attempt `timestamp`, `test`, `booking` (uid, status, UTC start/end, language, guest name/email/phone/notes/timezone, attendees, hosts from `booking_write_targets`, location, previous, cancelled_by, reason) and `event_type` (id, slug, title, timezone, team). Never tokens, credentials, resource names or calendar data. The phone follows `booking_guest_input` (only when the event type collects one).
- **Signing**: `X-Calrs-Signature: sha256=<hex HMAC-SHA256 of the raw body>` (`meeting::sign_hmac_sha256`, same scheme as the meeting and SMS webhooks), plus `X-Calrs-Event` and `X-Calrs-Delivery`. Replay protection is the signed `timestamp`, dedup the `id`. The secret (16 to 256 chars) is required, encrypted, keep-current on edit.
- **Delivery**: `KICK` (a `Notify`) wakes the task right after an enqueue. Retries after 1, 5, 15, 60, 240, 720 min (`MAX_ATTEMPTS` 7), then `failed`. One booking's events go out in order (a later one waits for the earlier). `start_pass` spawns one detached task per endpoint with something due (`IN_FLIGHT` prevents a second), at most `PER_ENDPOINT` rows per query and `ENDPOINT_BUDGET` per task, so a slow endpoint never holds the others; a failure pushes that endpoint's other due rows to the same retry. The endpoint is claimed before its rows are read, and each row is leased (`lease`: conditional UPDATE, also re-checking that the webhook is active with the same URL and secret) before it is sent; `attempts` is incremented in SQL. A reservation whose fill failed keeps its context (JSON in `payload`) and is filled by the next pass (`retry_unfilled`). Switching an endpoint off deletes its queue.
- **SSRF** (`check_target`, run on save and before every attempt): https only, http only for a host on the private-host allowlist; no userinfo; every resolved address public (`caldav::is_private_ip`, which now also unwraps IPv4-mapped IPv6) unless allowlisted; the checked addresses are pinned with `resolve_to_addrs`; no redirects; no proxy (`.no_proxy()`, a proxy would resolve the host itself); reqwest errors stored or logged with `without_url()` (URLs carry tokens); 10 s timeout; at most 1 KiB of an error body read and 200 chars kept. Tests pass the allowlist explicitly (`deliver_due_with`, `send_test_with`) instead of touching `CALRS_ALLOW_PRIVATE_HOSTS`.

## Known issues & TODOs

### Security
- ~~**CalDAV/SMTP passwords** stored as hex-encoded plaintext~~ — **Fixed in v0.10.0**: passwords are now encrypted at rest using AES-256-GCM. Key is auto-generated at `$DATA_DIR/secret.key` or provided via `CALRS_SECRET_KEY` env var. Legacy hex-encoded passwords are auto-migrated on startup.
- ~~**Passwords echoed to terminal**~~ — **Fixed in v0.10.0**: `prompt_password()` now uses `rpassword` for hidden input.

### Features not yet implemented
- Moving a booking to a host-chosen time through the REST API (v1 only asks the guest to pick one)

**Note:** delta sync is implemented. `commands/sync.rs` stores a per-calendar `sync-token` (migration 027) and `ctag`, issues an RFC 6578 `sync-collection` REPORT when it has one, and falls back to a full fetch when the server rejects it or reports an empty delta against a changed ctag. `--full` clears both. EWS sources keep their own flow.

### Test coverage roadmap
- **Web handler integration tests** — use `axum::test` with in-memory SQLite to test the full booking flow (create event type → fetch slots → book → confirm/cancel), dashboard renders, admin panel, token-based actions. Requires building a shared test harness (DB seed, AppState setup). This is the biggest coverage opportunity (~49% of the codebase is `src/web/`).
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

When adding a new migration (#333):
1. Run `scripts/new-migration.sh short_description`. It creates `migrations/YYYYMMDDHHMMSS_short_description.sql` (UTC timestamp); put the DDL in it.
2. Nothing to register: `build.rs` embeds every `.sql` file of `migrations/` (dotfiles skipped), sorted by name, as `MIGRATIONS`, which `db::migrate()` applies. It fails the build on a badly named file, on a new numbered one (`082_...`), on an uppercase `.SQL`, on two files sharing a timestamp, and on a migration with no SQL yet (it would be recorded as applied on your dev database).
3. **Never rename or delete a shipped migration.** `_migrations` records the file stem, so a renamed file runs again on every deployed database. The numbered files `001` to `081` keep their names forever; `legacy_migration_names_never_change` pins them.
4. **Merge order must match timestamp order.** A pending migration runs even when newer ones are applied, so if an older-timestamped migration merges after a newer one, fresh installs and upgraded databases apply the two in different orders (a table rebuild like 009 or 049 then drops a column on one and not the other). `scripts/check-migrations.sh origin/main` fails when a branch adds a migration older than the newest on `main`, or deletes or renames one; CI runs it on every PR and push. A PR's CI result can predate another merge, so run it again before merging a PR that adds migrations. When it fails, give the unmerged file a fresh timestamp (`git mv`); that is safe as long as the file never shipped in a tag.
5. **Maintenance branches (`1.23.x`).** They keep the hand-written list in `src/db.rs`. A migration needed there uses a timestamped name, the same file name and SQL on `main` and on the maintenance branch, and is registered by hand on the maintenance branch (a cherry-pick from `main` brings the file but not the registration). Never add `080_...` or later numbers there: `main` cannot take them back.

### Localization (Fluent)

calrs ships with translations for English, French, Spanish, Polish, German, Italian, Estonian, Brazilian Portuguese and Dutch, plus German in the formal register (`de-formal`, "Deutsch (Sie)"). Source files live under `i18n/{lang}/main.ftl` and are embedded in the binary via `include_str!` (no runtime files). The loader, language detection, and minijinja `t()` global are in `src/i18n.rs`. Templates use `{{ t("message-id", arg=value) }}` and the active language is injected into the rendering context as `lang` by the calling handler.

Both the guest side and the host side (dashboard, settings, forms, admin panel, auth pages) render through Fluent, including the bare error responses the booking flow returns without page chrome. **All nine locales are complete at 1480 keys**, held there by a test; the per-key English fallback still exists but nothing currently uses it.

**Emails are fully translated** (#209), each in its reader's language. Guest emails (confirmation, reminder, cancellation, pending, decline, expiry, reschedule, "pick a new time", and the copy sent to additional attendees, except to a watcher who claimed the booking, who reads theirs in their saved language) use the language stored with the booking (`bookings.language`, read back with `email::stored_booking_language` whenever someone other than the guest triggers the send). That stored value is exactly the language the booking form was shown in: each booking POST handler resolves its page `lang` the same way the GET form handler for that route does, and stores it, so the page, the emails and the SMS always agree. Page languages, by kind of page:
- **Public guest pages** (team profile, slots, form and booking POST; dynamic-group pages): the browser's `Accept-Language` (`i18n::detect_from_headers`) only. A logged-in user's saved preference never counts there: staff booking for a client, or an admin impersonating someone, would otherwise stamp their own language on a real guest's booking. `OptionalAuthUser` carries no language for that reason.
- **A host's own pages** (`/u/{username}` and the legacy single-user `/{slug}` routes): the same browser rule by default; the host's saved language when they chose "Always my language" in Profile & Settings (`users.public_page_language = 'host'`, per #76), falling back to the browser when they have none. `user_page_lang(host_language, mode, visitor)` is a pure function their GET and POST handlers call with the columns their own queries select. A booking POST validates its input before any query; the error pages it returns before reaching its own queries (captcha, rate limit, invalid input) look the page language up on that failure path only (`user_page_lang_lookup`, `legacy_page_lang_lookup`). Every error page a booking POST returns is in the page language (`render_booking_action_error*_in`).
- **Guest token pages** (cancel and reschedule forms and their result pages, including the "pick a new time" slot picker): the stored booking language (`token_page_lang`), else the browser's.
- **Host token pages** (approve and decline forms and results): the saved language of the booking's single host, the assigned member else the owner (`host_token_page_lang`, one query), else the browser's. A collective booking uses the browser's: every member gets the approval email in their own language, and the shared link cannot tell which one clicked. **Claim pages**: the language of the watcher the claim token was issued to (`claim_page_lang`), else the browser's. When the recipient has a saved language, the page matches the email that linked to it; without one the email is English and the page follows the browser.

Host, team member and watcher emails use the recipient's saved `users.language`, looked up by user id with `email::user_language(pool, user_id)` (never by address: one user's `booking_email` can be another user's login email). `booking_host_identity` returns the host's normalised language alongside the `(name, email)` it resolves, so callers need no second lookup; NULL, empty or unsupported means English, since a background send has no `Accept-Language` to fall back on. A collective booking emails each member in their own language. The invite email follows the inviter's resolved language (`auth_user.lang`), because nothing is known about the invitee and the personal message is the inviter's. The only English email left is the admin SMTP test. Dates in email subjects and bodies are written out in the reader's language with `long_date` (`src/email/render.rs`, over `i18n::format_long_date`), exactly as the booking pages show them ("Dienstag, 27. Oktober 2026"); host emails convert to the host's timezone first, then write the date out, and watcher and claim emails to the recipient's own timezone (`recipient_date_time`), so the weekday, the time and the zone label always come from one zone. Deadlines (the approval reminder's, an invite's expiry) are written out the same way by `deadline_display`, with the zone named. Polish takes the genitive month in a date ("10 kwietnia") while calendar headers keep the name ("kwiecień 2026"): `format_long_date` also passes `$monthnum`, which only the Polish pattern selects on. The ICS attachments and SMS bodies keep `YYYY-MM-DD` (SMS is billed per segment). New emails go through the `compose_*` pattern in `src/email/guest.rs` and `src/email/host.rs`: a pure function building an `EmailParts` (subject, heading, message, rows, plain-only lines, footer, buttons) in one language, so tests can read exactly what the recipient gets, and `send_composed` for delivery.

Three helpers stay English on purpose: the CSRF rejection, the 500 page, and the OIDC failure. They live in helpers called from ~240 sites with no `lang` in scope, and they are diagnostics rather than flow messages.

Every locale except French addresses the reader informally, matching what the earliest translations chose: du, tu (Italian), tú, ty, você, sa, je. French uses "vous" throughout, the usual register for French software; keep new French strings formal so the file stays consistent.

**Formal German (`de-formal`).** B2B deployments in German-speaking countries address customers as "Sie", so German ships twice: `de` (du) and `de-formal` (Sie, labelled "Deutsch (Sie)"). `de-formal` is an overlay: it only carries the messages that address the reader (about 200), every other key is read from `de`. A browser alone never selects it (`Accept-Language` only says `de`). Two ways in: a user picks it in Profile & Settings, which sets their dashboard and emails, and with "Always my language" their public pages; or the admin sets "German form of address" to Sie (System settings, `auth_config.german_formal`, migration 073), and every page that would pick `de` from the browser picks `de-formal` instead. That travels as an internal request header: `german_formal_middleware` (`src/web/mod.rs`) strips any client copy of `i18n::GERMAN_FORMAL_HEADER` and sets it from `AppState.german_formal`, and `detect_from_headers` reads it, so the ~40 call sites need no state. A saved user language is never rewritten. The bookings made in it store `de-formal`, and the guest emails follow. `html_lang` declares it as `de`. A key it lacks falls back to `de`, then English (`parent_lang`). When you add a German string that addresses the reader, add its "Sie" form to `de-formal`; other German strings need nothing there. `german_formal_addresses_the_reader_formally` checks what a "Sie" reader actually sees (the override, else the inherited `de` text) and fails on any du/dich/dir/dein, du-only verb form or informal imperative ("Wähle" instead of "Wählen Sie"); `german_formal_overrides_match_german` fails on an override whose key is gone from `de`, whose placeables differ, or which merely copies `de`.

**Where `lang` comes from on host pages.** The `AuthUser` and `AdminUser` extractors resolve it once, in `src/auth.rs`, from the user's saved preference then `Accept-Language`. A dashboard handler passes `lang => auth_user.lang` and nothing else. Pre-login pages (login, register) have no user row, so they call `i18n::detect_from_headers` directly.

**Keys that reach Fluent through a variable.** Most call sites name their key inline, as `translate(lang, "key", None)`. Two places on the booking path do not: `render_booking_action_error_keys_in(state, lang, title_key, body_key)` takes a `bae-*` pair, and the booking-form validators return their key as `Err("validate-*")` for the handler to resolve. The email module's short helpers (`t(lang, "key")`, `ta(lang, "key", ..)`, `push_row(rows, lang, "key", ..)`, `status_subject(..)`) are scanned too. Both forms are covered by the Rust key guard, so a typo still fails the build rather than rendering the raw id to a guest. Add any further indirection to `find_rust_keys` in `src/i18n.rs` at the same time you add the indirection itself.

**Guard tests** in `src/i18n.rs`: every `t()` key referenced from a template exists in the English bundle; the same for keys used from Rust (including the two indirect forms above); every template still loads; every locale covers every English key; and plural messages carry the categories the locale's grammar needs. That last one matters because Polish selects one/few/many for integers, so a translation that copies English's one/other reads wrong at 2 and at 5. Two further guards in `src/web/tests/guards.rs` (they scan every shipping `.rs` file under `src/web/`): one fails if a host-facing template is rendered without a `lang` in its context, the other fails if any call site hands the booking error page a bare English sentence instead of a key or an already-translated value.

**Numbers passed to `t()`** reach Fluent as numbers, not strings, so `{ $count -> [one] ... }` plural selectors work. Grouping is switched off, so an integer renders as it always did ("1440", not "1,440").

**Strings a page composes at runtime** (JS building a summary hint or a search result) cannot use Fluent arguments, because the values only exist after the visitor acts. Those keys use `%1`/`%2` placeholders substituted client-side, collected in one object per page (`ETF_I18N`, `ADMIN_I18N`) built with `{{ t('key') | tojson }}`. Prefer real Fluent arguments everywhere else.

**Literal braces in a Fluent value** must be escaped as `{"{"}`: a bare `{` starts a placeable. This bites the meeting-pattern help, which documents `{username}` and `{random}` tokens.

**Branch workflow (long-lived `i18n` branch).** The `i18n` branch is permanent. **Do not delete it after merging.** Translation contributions arrive as pull requests against it. Periodically (e.g. before each release) merge `i18n` into `main`, then continue using the same branch for the next round. The branch never gets recreated, and it normally sits exactly at `main`, so a round never starts stale.

**There is no translation platform.** A Hosted Weblate project was applied for and never approved, so the account was closed and the links 404ed for months while the README still advertised them (#200). No commit in this repository's history was ever authored by Weblate. Every community translation calrs has received came in as an ordinary pull request. Do not reintroduce a platform reference without a working project behind it.

**When you add or change a translatable string:**
1. Land it on the `i18n` branch first, not `main`. This keeps half-translated UI off `main` and gives contributors a window before the next merge.
2. Add the new key to `i18n/en/main.ftl` (the source of truth), then to every other locale. The runtime still falls back to English per missing key, but the coverage test does not let you rely on it.
3. If the change touches a template that wasn't translated yet, convert its hard-coded strings to `{{ t("...") }}` calls in the same commit, and add render-site context entries (`lang => crate::i18n::detect_from_headers(&headers)` for guest pages, `lang => auth_user.lang` for authenticated dashboard pages).
4. Run `cargo test i18n::` before pushing: it checks coverage across all nine locales and the plural categories each language needs.

**Adding a key now costs nine translations, not one.** The coverage test fails until every locale has a value. That is deliberate: a half-translated page is worse than an English one because nobody notices it is wrong. If you cannot supply all nine, land the key on `i18n` and leave it there until someone can.

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
