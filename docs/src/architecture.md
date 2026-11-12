# Architecture

calrs is a Rust executable that runs either operator CLI commands or an Axum web server (`calrs serve`). SQLite stores application state, credentials are encrypted with an instance key, and Minijinja renders the HTML interface. The executable serves embedded static assets and translations; deploy its matching `templates/` directory alongside it.

For the file-by-file contributor map, see [`ARCHITECTURE.md`](https://github.com/olivierlambert/calrs/blob/main/ARCHITECTURE.md) in the repository. This chapter describes the main boundaries and runtime behavior.

## Request and booking flow

1. Public booking routes resolve a personal, team, invite or dynamic-group event type.
2. Availability combines event rules, date overrides, selected calendar events, existing bookings, time off, participant hours, booking limits and resources.
3. Submitting a booking revalidates access and availability before storing it. A pending request can hold the time; confirmation re-checks conflicts.
4. Confirmed bookings are written to configured calendar destinations, with generated meeting links where selected. Notifications and lifecycle webhooks follow the booking transition.
5. Token-based routes support guest cancellation/rescheduling and host approval/decline. Signed-in routes enforce the corresponding host and management permissions.

Bookings store UTC endpoints for new rows. Legacy rows retain their previous storage interpretation. Weekly rules remain local to the event's named timezone; each participant's working hours are interpreted in their own timezone.

A process-wide booking lock serializes the availability checks and booking writes that claim time. Network refreshes happen before the lock, and a database transaction wraps the writes. A partial unique index additionally prevents duplicate starts per event type and assigned member. The lock does not coordinate separate server or CLI processes; see [Security](./security.md#double-booking-prevention).

## Main components

| Component | Responsibility |
|---|---|
| `src/main.rs`, `src/commands/` | CLI, startup and background task launch |
| `src/web/` | Routes, public pages, dashboard, availability, booking transitions and integration endpoints |
| `src/providers/`, `src/caldav/`, `src/ews/` | Common calendar interface, CalDAV and Exchange EWS |
| `src/microsoft_oauth.rs`, Microsoft provider | Delegated Microsoft Graph calendars and token handling |
| `src/oauth2_caldav.rs`, `src/google_calendar.rs`, `src/google_meet.rs` | Google calendar OAuth, out-of-office snapshots and conferences |
| `src/auth.rs`, `src/mfa.rs`, `src/oauth.rs` | Local/OIDC sign-in, TOTP and the OAuth server used by API/MCP clients |
| `src/email/`, `src/sms/` | Notification rendering, transport and SMS gateway adapters |
| `src/resources.rs`, `src/time_off.rs`, `src/rrule.rs` | Shared resources, absences and recurrence expansion |
| `src/crypto.rs`, `src/net_guard.rs`, `src/trusted_proxies.rs` | Credential encryption, outgoing-request address checks and proxy trust |
| `src/db.rs`, `migrations/`, `build.rs` | SQLite and embedded, ordered migrations |
| `templates/`, `assets/`, `i18n/` | HTML, bundled browser assets and Fluent translations |

## Storage

SQLite runs in WAL mode with foreign keys enabled. Schema migrations run automatically on startup. Numbered historical migration names remain unchanged; newer migrations use timestamped filenames and are embedded at build time.

| Tables | Purpose |
|---|---|
| `users`, `accounts`, `sessions` | Identity, scheduling ownership and sessions |
| `caldav_sources`, `calendars`, `events` | Provider connections and cached events; historical table names also hold non-CalDAV sources |
| `event_types`, `availability_rules`, `availability_overrides` | Meeting configuration and hours |
| `event_type_calendars`, `event_type_write_targets`, `booking_write_copies` | Conflict selection, per-host write destinations and recorded calendar copies |
| `bookings`, `booking_attendees`, `booking_invites` | Booking state, guests and invitation access |
| `event_type_questions`, `booking_answers` | Form questions and snapshots of submitted answers |
| `teams`, `team_members`, `team_groups`, `groups`, `user_groups` | Scheduling teams and OIDC-derived identity groups |
| `user_time_off`, `booking_frequency_limits` | Absences and caps |
| `event_type_favorites` | Personal directory shortcuts |
| `webhooks`, `webhook_deliveries`, `sms_usage` | Outgoing integration queues and SMS accounting |
| `auth_config`, `smtp_config` | Instance policies and notification configuration |

Cached events are unique by UID, calendar and recurrence instance. Credential encryption depends on `secret.key` or `CALRS_SECRET_KEY`; [backups](./deployment.md#backup) must preserve the key and uploads as well as the database.

## Calendars and background work

CalDAV uses discovery (`PROPFIND`), event queries and delta sync (`REPORT`), plus `PUT`/`DELETE` for bookings. XML is parsed by namespace rather than by hard-coded prefixes. EWS performs SOAP operations; Microsoft Graph reads calendar snapshots and writes appointments through its native API. Google uses OAuth-backed CalDAV together with the Calendar API for out-of-office events, attendee updates and Meet.

Three background activities run separately:

- Calendar sync schedules due sources every minute, with bounded concurrency and at most one background request sequence per server. See [background sync](./deployment.md#background-calendar-sync).
- The notification loop handles reminders, pending-request expiry, cleanup and calendar write-back retries.
- The outgoing webhook task delivers queued events when woken and on a periodic pass, with retry limits and signed payloads.

A failed sync retains cached data and reports an error. A failed availability read while accepting a booking refuses the request rather than treating missing data as free. Those are distinct failure modes. See [CalDAV](./caldav.md#when-a-sync-fails) and the provider guides for recovery and snapshot limits.

## Interfaces and authentication

| Interface | Authentication and scope |
|---|---|
| Dashboard and admin pages | Server-side session, role/ownership checks and CSRF on browser forms |
| Public booking pages | Public event access or the required invitation tokens |
| Guest/host action links | Sensitive action tokens plus booking-state checks |
| `/api/v1` | Scoped personal access token or OAuth Bearer token; session cookies do not authenticate it |
| `/api/v1/openapi.json` | Public OpenAPI description; contains no booking data |
| `/api/docs` | Signed-in interactive reference; API operations still require a Bearer token |
| `/mcp` | OAuth Bearer tokens for the MCP resource, Streamable HTTP |
| CLI | Direct access to the data directory, outside browser-session authorization |

OIDC sign-in identifies returning users by their stored subject. New account registration or linking requires the verified-email rules described in [Authentication](./authentication.md). Calendar OAuth consent and API/MCP grants are separate from OIDC login.

## Rendering and notifications

Minijinja templates use shared layouts and card/list components. Browser interaction uses vanilla JavaScript; there is no Node.js production runtime. Fluent translations and locale-aware formatting cover the dashboard, public flow and notifications. Browser assets are bundled; optional integrations such as an externally hosted captcha can introduce additional requests.

SMTP supports STARTTLS, implicit TLS and a trusted local relay. Notifications contain HTML and plain text; calendar attachments and provider copies use the booking UID and sequence to track updates. Guest-facing and host-facing content are separate: host notes never enter guest messages, calendar invitations or webhook payloads. Question answers follow their documented [visibility rules](./booking-questions.md#where-the-answers-appear).

## Validation and contributions

CI checks formatting, templates, migration order, Clippy, tests and coverage. Tests cover scheduling, recurrence, timezone transitions, authorization, migrations, providers, APIs, notifications and translations. Browser verification scripts live in [`scripts/`](https://github.com/olivierlambert/calrs/tree/main/scripts).

```bash
cargo fmt --check
cargo clippy --profile=test
cargo test
mdbook build docs
```

For a source-only documentation change, build the book and check its internal links. For application changes, run the checks relevant to the affected behavior. Translation contributions target the permanent `i18n` branch; see the [contributor instructions](https://github.com/olivierlambert/calrs#contributing-a-translation).
