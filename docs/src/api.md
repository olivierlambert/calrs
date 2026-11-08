# REST API

calrs has a small JSON API under **`/api/v1`** for integrations: list your
event types, read their bookable times, read the bookings you host, and book,
cancel or ask for a new time on a guest's behalf. It runs inside
`calrs serve`; there is nothing to install or enable.

The API is deliberately small. Everything it does, the dashboard or the
booking page already does, through the same code: a booking made through the
API is checked, stored, emailed, written to calendars and texted exactly like
one made on the booking page.

## Authentication

Every request carries a Bearer token:

```
Authorization: Bearer <token>
```

Cookies never authenticate the API, so a page you visit cannot use your
signed-in session to call it (there is no CSRF to defend against). Requests
from a browser page of another origin (an `Origin` header that is not the
calrs origin) are refused, and the API sends no CORS headers.

There are two kinds of token.

### Personal access tokens

For your own scripts. Create one at **Profile & Settings → API tokens**
(`/dashboard/settings/api-tokens`): give it a name, choose its permissions and
when it expires (7, 30, 90 or 365 days, or never). The token is shown **once**,
when it is created; calrs stores only its SHA-256 hash. It looks like
`calrs_pat_` followed by 43 characters. The page lists your tokens (name,
the first characters, permissions, creation, expiry, last use) with a
**Revoke** button. A user holds at most 20.

By default only administrators can create personal access tokens: a token is a
long-lived password for the API, so letting every user mint them is the
operator's call. **Admin panel → Access → API tokens → Let all users create
API tokens** opens it to everyone. Turning it off again stops the tokens of
users who are not administrators at once (they work again if it is turned
back on); an administrator who is demoted loses the use of theirs the same
way. OAuth applications (below) are not affected by the switch.

### OAuth applications

For third-party applications acting for many users, calrs's OAuth server (the
one MCP clients use, see [MCP integration](./mcp.md)) issues tokens for the
API as a second resource:

- **resource**: `<base-url>/api/v1` (metadata at
  `/.well-known/oauth-protected-resource/api/v1`; the 401 challenge names it).
- **scopes**: the four below. Omitting `scope` requests all four.

Registration, consent, PKCE, token lifetimes (one-hour access tokens, rotating
refresh tokens, 30-day grants) and revocation work as described for MCP. A
grant is bound to its resource: an MCP token does not open the API, an API
token does not open `/mcp`, and the scopes of one resource cannot be requested
for the other. API grants appear under **Profile & Settings → Connected apps**
with their permissions, and are revoked there.

### The rules both follow

- A token acts as its user, and the user is checked again on every request:
  a disabled account is refused at once.
- **Two-factor authentication**: a token is worth what the session that made
  it was worth. When the admin requires MFA, a token created (or a grant
  approved) by a local account from a session that had not passed it stops
  working, as that session does.
- Whatever signs a user out everywhere also deletes their tokens: suspending
  the account, `calrs user set-password`, `calrs user reset-mfa`, enrolling,
  disabling or regenerating two-factor authentication, linking to SSO.
  Re-enabling a suspended account brings no token back.
- The admin panel's per-user **Revoke connected apps** also deletes the
  user's API tokens, and its count includes them.
- An admin impersonating a user cannot create or revoke that user's tokens.

### Scopes

| Scope | Allows |
|---|---|
| `event-types:read` | `GET /event-types`, `GET /event-types/{id}` |
| `slots:read` | `GET /event-types/{id}/slots` |
| `bookings:read` | `GET /bookings`, `GET /bookings/{id}` |
| `bookings:write` | `POST /bookings`, `POST /bookings/{id}/cancel`, `POST /bookings/{id}/request-reschedule` |

A missing scope answers 403 with `insufficient_scope`, and a
`WWW-Authenticate` header naming the scope needed.

## Who can see and do what

The API gives a user no more than the dashboard does, and an administrator no
more than any other user: like MCP, there is no global admin bypass.

- **Event types** you *manage*: the personal ones you own, and the team ones of
  teams you administer. Internal event types the booking directory offers you
  are listed too, read-only (`"manageable": false`): no slots, no bookings.
- **Slots** and **booking for a guest**: event types you manage.
- **Bookings** you *host*: the bookings of event types you own, the bookings a
  round-robin team assigned to you, collective bookings of your team (unless
  you are excluded from that event type), and bookings you claimed as a
  watcher. That is the rule of host notes, which these readers may see.
- **Cancel** and **request a reschedule**: as on the dashboard, the owner of
  the event type. Another host gets 403.

Anything else answers 404, whether it exists or not.

Never returned: credentials, the booking's cancel, reschedule or approval
tokens, other users' data, and anything from your calendars beyond what the
slot computation makes of it (no event titles, no calendar names).

## Conventions

- JSON only. Request bodies need `Content-Type: application/json` (415
  otherwise) and are limited to 16 KiB (413). Unknown fields and unknown
  query parameters are refused (400).
- Times are RFC 3339. calrs answers in UTC (`2026-11-10T08:00:00Z`).
- Every answer carries `Cache-Control: no-store`.
- Lists are paged with an opaque cursor: pass `next_cursor` back as `cursor`
  until it is `null`. A page can be short, even empty, and still have a next
  cursor (permission and time filters run after the page is read).
- Errors always have this shape:

```json
{"error": {"code": "slot_unavailable", "message": "This slot is no longer available."}}
```

`code` is stable; `message` is English text for people and may change.

| Status | Codes |
|---|---|
| 400 | `invalid_request`, `invalid_json`, `invalid_start`, `invalid_timezone`, `invalid_phone`, `invalid_attendees`, `invalid_range`, `invalid_cursor`, `invalid_idempotency_key` |
| 401 | `unauthorized` |
| 403 | `insufficient_scope`, `forbidden`, `origin_not_allowed` |
| 404 | `not_found` |
| 405 | `method_not_allowed` |
| 409 | `slot_unavailable`, `limit_reached`, `booking_not_active`, `no_bookable_dates`, `idempotency_in_progress` |
| 413 | `payload_too_large` |
| 415 | `unsupported_media_type` |
| 422 | `too_soon`, `outside_booking_window`, `booking_refused`, `idempotency_key_reused` |
| 429 | `rate_limited` (with `Retry-After`) |
| 500 | `internal_error` |

### Rate limits

- 300 requests per minute per token (per OAuth grant, or per personal token).
- On top of that, slot reads and every `POST`: 60 per 5 minutes per user,
  shared by all of the user's tokens.

## Endpoints

### `GET /api/v1/event-types`

Query: `cursor` (optional).

```json
{
  "event_types": [
    {"id": "6c1e4f0a-...", "slug": "intro", "title": "Intro call",
     "duration_min": 30, "visibility": "public", "team_id": null,
     "manageable": true}
  ],
  "next_cursor": null
}
```

### `GET /api/v1/event-types/{id}`

```json
{
  "event_type": {
    "id": "6c1e4f0a-...", "slug": "intro", "title": "Intro call",
    "description": "A first call.", "duration_min": 30,
    "visibility": "public", "team_id": null, "manageable": true,
    "requires_confirmation": false, "location_type": "link",
    "timezone": "Europe/Paris", "scheduling_mode": null,
    "max_additional_guests": 2, "phone": "off"
  }
}
```

`scheduling_mode` is `round_robin` or `collective` for a team event type.
`phone` says whether a booking asks for the guest's phone number (`off`,
`optional`, `required`). The location's value (a meeting link, an address)
is not returned here.

### `GET /api/v1/event-types/{id}/slots`

Query: `from` and `to` (dates, `YYYY-MM-DD`, inclusive, at most 31 days),
`timezone` (IANA name, default the event type's). The days are the guest's
days in that timezone.

The slots are computed by the booking page's own engine, after the same
on-demand calendar sync: weekly hours and date overrides, minimum notice,
buffers, the booking horizon and bookable dates, frequency limits, time off,
synced calendar events and bookings (pending requests that hold their slot
included), shared resources, and for a team event type its members by
scheduling mode (any member free for round robin, all for collective).

```
GET /api/v1/event-types/6c1e4f0a-.../slots?from=2026-11-10&to=2026-11-10&timezone=Europe/Paris
```

```json
{
  "event_type_id": "6c1e4f0a-...",
  "timezone": "Europe/Paris",
  "from": "2026-11-10",
  "to": "2026-11-10",
  "slots": [
    {"start": "2026-11-10T08:00:00Z", "end": "2026-11-10T08:30:00Z",
     "date": "2026-11-10", "time": "09:00"}
  ]
}
```

`date` and `time` are the slot in the requested timezone; pass `start` back
to book it.

### `GET /api/v1/bookings`

Query: `cursor`, `status` (`pending`, `confirmed`, `cancelled`, `declined`),
`from` and `to` (RFC 3339; bookings starting at or after `from`, before `to`).
Ordered by start time.

```json
{
  "bookings": [
    {
      "id": "1f6a...", "uid": "0b7d2e0e-...@calrs", "status": "confirmed",
      "start": "2026-11-10T08:00:00Z", "end": "2026-11-10T08:30:00Z",
      "created_at": "2026-11-07T09:12:44Z",
      "event_type": {"id": "6c1e4f0a-...", "slug": "intro", "title": "Intro call", "team_id": null},
      "guest": {"name": "Jeanne Martin", "email": "jeanne@example.com",
                "phone": null, "timezone": "Europe/Paris", "language": "fr",
                "notes": "About the renewal"},
      "attendees": ["paul@example.com"],
      "location": {"type": "link", "value": "https://meet.example.com/abc"},
      "host_notes": "Prefers mornings."
    }
  ],
  "next_cursor": null
}
```

`uid` is the iCalendar UID of the invitation (the one webhooks carry).
`host_notes` are the hosts' own notes on the booking.

### `GET /api/v1/bookings/{id}`

`{"booking": {...}}`, the same object.

### `POST /api/v1/bookings`

Books an event type you manage for a guest.

```json
{
  "event_type_id": "6c1e4f0a-...",
  "start": "2026-11-10T08:00:00Z",
  "timezone": "Europe/Paris",
  "name": "Jeanne Martin",
  "email": "jeanne@example.com",
  "notes": "About the renewal",
  "phone": "+33 6 12 34 56 78",
  "attendees": ["paul@example.com"],
  "language": "fr"
}
```

- `start` must be one of the event type's slots (as `GET .../slots` lists
  them); `timezone` is the guest's (default the event type's), stored with the
  booking and used in the guest's emails.
- `notes`, `phone`, `attendees` and `language` are optional. `phone` follows
  the event type's phone setting (required, optional, or ignored) and is
  stored in E.164. `attendees` is limited by the event type's additional
  guests setting. `language` is the language of the guest's emails and SMS;
  by default the language you set in calrs, else English.
- No invite is needed for internal or private event types: you manage them.
  None is used up either.

The booking goes through the booking page's own code: the same validation, a
re-check of the time against the slot engine and of the shared resources
under the booking lock, the same pending-or-confirmed decision (an event type
that requires approval creates a pending request, and the host is asked to
approve it), the same emails to the guest, the attendees and the hosts, the
same calendar write-back, meeting link, watcher notifications and SMS.

`201 Created`:

```json
{"booking": {"id": "1f6a...", "status": "confirmed", "...": "..."}}
```

The booking object is the one `GET /bookings/{id}` returns, without
`host_notes`. A team admin who books a round-robin event type assigned to
another member is not one of that booking's hosts: they get it back here,
but `GET /bookings/{id}` answers 404 for them afterwards.

**`Idempotency-Key`**: send a key (1 to 255 visible ASCII characters) to make
retries safe. The first answer with that key is kept 24 hours, per user, and
replayed for a retry with the same body (with `Idempotency-Replayed: true`);
the same key with another body answers 422 `idempotency_key_reused`, and a
retry while the first request still runs 409 `idempotency_in_progress`.
Server errors are not kept, so the request can be retried with the key.

Refusals: 409 `slot_unavailable` (the time is not, or no longer, offered),
409 `limit_reached` (a booking frequency limit), 422 `too_soon` (inside the
minimum notice), 422 `outside_booking_window`, 400 for invalid input.

### `POST /api/v1/bookings/{id}/cancel`

Body (optional): `{"reason": "Double booked"}`.

What the dashboard's cancel button does: a confirmed booking is cancelled, a
pending request declined; the calendar copies and resource reservations are
deleted, the guest, the attendees and the host are emailed (with the reason),
the guest texted. Answers `{"booking": {...}}` with its new status. A booking
already ended answers 409 `booking_not_active`.

### `POST /api/v1/bookings/{id}/request-reschedule`

Body: none, or `{}`.

What the dashboard's reschedule button does: the guest gets a new link and the
"pick a new time" email, and the booking keeps its time until they choose
one. 409 `no_bookable_dates` when the event type has no bookable day left.
Moving a booking to a time you choose is not in v1.

## Example

```bash
TOKEN=calrs_pat_...
BASE=https://cal.example.com/api/v1

curl -s -H "Authorization: Bearer $TOKEN" "$BASE/event-types"

curl -s -H "Authorization: Bearer $TOKEN" \
  "$BASE/event-types/$ET/slots?from=2026-11-10&to=2026-11-14&timezone=Europe/Paris"

curl -s -X POST -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" -H "Idempotency-Key: crm-4711" \
  -d '{"event_type_id":"'$ET'","start":"2026-11-10T08:00:00Z","timezone":"Europe/Paris","name":"Jeanne Martin","email":"jeanne@example.com"}' \
  "$BASE/bookings"
```

## Versioning

The version is in the path. Within v1, fields may be added to answers and new
endpoints may appear; nothing is removed or changes meaning. A change that
would break a client gets `/api/v2`.

## Tests

```bash
cargo test web::tests::api
```
