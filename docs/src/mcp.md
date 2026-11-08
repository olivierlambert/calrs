# MCP integration

calrs exposes a [Model Context Protocol](https://modelcontextprotocol.io/) server
at **`/mcp`**, using Streamable HTTP. It runs inside `calrs serve`, in the same
binary and process as the web application.

## Connect

1. In your MCP client, add a remote **Streamable HTTP** server with the URL
   `https://your-calrs-host/mcp`.
2. Choose OAuth authentication (or let the client discover it automatically).
3. Sign in using your normal calrs password or your existing SSO provider.
4. Review the application name, callback address and permissions, then approve.

There is no API token to copy, client secret to create, or new setting to enable.
The client registers itself and manages its tokens. Use a client supporting OAuth
discovery, **dynamic client registration**, and S256 PKCE. Clients supporting only
manually configured credentials or Client ID Metadata Documents cannot connect
automatically to this implementation.

The server uses the **existing public base URL** (`CALRS_BASE_URL`, or the saved
Base URL setting), just like email links and SSO. Production deployments should
already have this set to their public HTTPS origin. Without it, discovery uses
`http://localhost:3000` for development. Custom local ports use the same existing
Base URL setting. Deploy at the origin root, as with the web application.

The endpoint is intended for native and server-side MCP clients. Cross-origin
browser JavaScript is not enabled: supplied `Origin` headers must match the calrs
origin. Reverse proxies must forward `Authorization` and `MCP-Protocol-Version`.

## Tools

### `list_event_types`

Arguments: `{}` initially, then `{"cursor":"<next_cursor>"}` for subsequent pages.

Returns enabled event types the connected user can distribute:

- Personal event types they own.
- Team event types they manage as a team admin.
- Internal event types the booking directory offers them, which any
  authenticated colleague can distribute.

Global administrators get no extra reach here: an admin's integration sees and
distributes only what the admin owns or manages as a team admin, plus the
directory's internal types, like the busy-time tool, which reads only the
admin's own calendar. The dashboard keeps the admin's full access.

Each item has `id`, `slug`, `title`, `duration_min`, `visibility`, and `team_id`.
Use the **ID**, not the slug, when creating an invite. Slugs can be shared by
different users or teams. Follow `next_cursor` until it is `null`; permission
filtering can produce a short or empty page that still has a next cursor.

### `create_invite_link`

Arguments:

```json
{"event_type_id":"<id from list_event_types>","expires_days":30}
```

Example result:

```json
{
  "url": "https://cal.example/u/alice/intro?invite=<token>",
  "expires_at": "2026-10-23T12:00:00Z",
  "max_uses": 1
}
```

For internal and private events, `expires_days` accepts **7**, **14**, **30**, or **0**
(never), matching the UI. Omit it to default to **seven days**. Never-expiring
invites return `expires_at: null`. Each link can be used for **one booking**. Guests
enter their own name and email. Team links use `/team/{team}/{event}?invite=...`.
The tool creates the same invitation records as the dashboard quick-link action;
it sends no email and creates no booking. You can manage the resulting internal
and private invites on the dashboard's invite-management page.

Internal events are available to any authenticated user. Private and public
events require owner or team-admin permissions. Disabled events
are rejected. Public events return their ordinary **reusable booking URL**, with
`expires_at: null` and `max_uses: null`, without inserting an invitation record;
`expires_days` does not affect these public URLs.
For public events belonging to a private team, the URL includes that team's
existing reusable access token. Generating a link does not change visibility.

Each call for an internal/private event creates a **new** link. Avoid automatically
repeating a successful call. Link requests are limited to ten attempts per user
per five minutes, tool calls to 120 per user per minute, and other MCP requests
(`initialize`, `ping`, `tools/list`) to 600 per user per minute (HTTP 429
beyond).

Example request after MCP initialization:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "create_invite_link",
    "arguments": {"event_type_id": "<event-type-id>"}
  }
}
```

Successful tool results include both JSON text in `content` and the object in
`structuredContent`. Permission/argument/database failures are tool results with
`isError: true`; missing OAuth scope produces HTTP 403 with a scope challenge.

### `get_calendar_blocks`

Arguments:

```json
{"start":"2026-10-19T00:00:00Z","end":"2026-10-20T00:00:00Z"}
```

Both times must be RFC3339 timestamps with a timezone offset. The end is exclusive
and must be after the start, with a maximum range of **31 days**.

The tool returns `blocks` (merged, clipped UTC intervals), `cached: true`, and
`sources` with `id`, `last_synced`, and `sync_failed` for the user's enabled
calendar sources. It uses the same calendar/recurrence and booking calculations
as availability, including pending bookings that hold their slot. Transparent
and cancelled calendar events do not block time.

This reads **only the connected user's** busy times, even when that user is an
administrator. Event titles, descriptions, attendees and credentials are not
returned. The tool reads the current sync cache without contacting remote
calendars; inspect source freshness/failures before relying on it. A database
failure is reported as an error, not as a free calendar.

These are busy intervals, not bookable slots: working hours, per-event-type
buffers, notice periods, shared resources, and team scheduling rules still apply
on the booking page.

## What the model receives

The tools are read-only, except `create_invite_link`, which creates a
single-use invitation (or returns a public booking URL) and nothing else: no
tool books, cancels, reschedules or emails anyone. What they return reaches the
AI model as text, including event type titles that colleagues wrote (their
internal meetings appear in `list_event_types`). Treat that text as untrusted
input: a title can carry instructions aimed at the model (prompt injection).
The worst such a title can obtain is what the tools allow, listing event types,
reading your busy times and creating invite links in your name, but clients
that also hold other tools (email, files, the web) should ask before acting on
what a calrs result says.

## OAuth behavior

The browser approval page lists the requested scopes:

| Scope | Tool |
|---|---|
| `event-types:read` | `list_event_types` |
| `calendar:read` | `get_calendar_blocks` |
| `invites:write` | `create_invite_link` |

Clients may request a subset; omitting scope requests all three. Authorization
permits only the listed scopes. On refresh, scope order does not matter. A
narrower request limits the new access token, while the grant and replacement
refresh token retain their original permissions. Omitting scope on refresh uses
the original grant permissions. Existing access tokens keep their own scopes.
Authorization
and token requests must include `resource=<public-base-url>/mcp`. Public clients
use `token_endpoint_auth_method=none`, authorization code with **S256 PKCE**, and
a registered HTTPS or loopback HTTP callback URI (ASCII only). The callback of
an authorization must equal a registered one as a parsed URL (so
`https://app.example` and `https://app.example/` are the same, the path and
query are exact); a loopback callback may use any port (RFC 8252). Registrations
asking for other grant types too (VS Code lists the device flow) get the two
supported ones back.

Access tokens last at most **one hour**. Refresh tokens rotate on each exchange
and expire **30 days after approval**; reconnect then. Clients must serialize
refreshes and save the replacement token: replay of a spent refresh token revokes
the whole grant. Tokens and codes are stored as SHA-256 hashes. Authorization
codes are single use and expire after two minutes. Disabled users lose access
immediately, and permissions are checked again on every tool call. Admin
impersonation does not change the identity granting OAuth access.

Grants follow the same rules as browser sessions:

- Approving requires a fully signed-in session. With local two-factor
  authentication enrolled, or required by the admin, that means the code was
  entered; a login that stops at the code prompt resumes the approval after it.
- A grant made before the admin required two-factor authentication, by a local
  account that had not passed it, is deleted with its tokens when the
  requirement is turned on, as that account's sessions are. Turning the
  requirement off again does not bring it back.
- Everything that signs a user out of every session also deletes their grants
  and tokens: suspending the account, `calrs user set-password`,
  `calrs user reset-mfa`, enrolling, disabling or regenerating two-factor
  authentication, and linking the account to SSO. Re-enabling a suspended
  account does not bring old tokens back; the client has to be approved again.

The approval page cannot be framed (`frame-ancestors 'none'`, `X-Frame-Options:
DENY`), its form is CSRF protected, and it shows the callback address the code
will be sent to. Client names with control, bidirectional or invisible characters
are refused at registration. The callback is shown as the browser will follow
it (parsed: a backslash, a percent-encoded or a numeric host are written out),
with its host on a line of its own.

Clients can disconnect using `POST /oauth/revoke` with form fields `client_id`
and `token` (access or refresh token). This revokes the whole grant. Logging out
of the browser does not revoke an integration's independent grant.

Users see their grants under **Profile & Settings → Connected apps**
(`/dashboard/settings/apps`): the application name, the callback it was
approved for, the permissions in plain words, the day it was approved and the
day it was last used. **Revoke** deletes
the grant with its tokens at once. Admins see, in the action menu of a user in the
admin panel, how many applications that user has connected, with an action that
revokes all of them.

The OAuth endpoints are rate limited per client address (the rightmost
`X-Forwarded-For` value, as for bookings; an IPv6 address counts by its /64):
client registration 30 requests per 15 minutes, authorization, token exchanges
and revocations 60 requests per 5 minutes each. One address cannot hold more
than 50 clients registered in the last 30 days that nobody approved. Over a
limit they answer HTTP 429 with `{"error":"temporarily_unavailable"}`.

The client address follows the [trusted proxy](security.md#trusted-proxies)
rule: `X-Forwarded-For` is believed only from a trusted proxy, otherwise the
connection's own address is used. A trusted proxy that sends no
`X-Forwarded-For` makes every request look like the proxy: the rate limits then
apply to everyone together, and the per-address client cap is not applied at
all (it would let one registrant lock everyone out).

A client nobody approved is deleted after 30 days, so a user who denied, or gave
up during login or two-factor setup, can retry with the same client. A client
once approved is kept for 180 days after its last approval, even when its grant
was revoked or expired, so the MCP client can simply authorize again. At most
10,000 clients are registered: at that cap the oldest client in either of those
deletable states makes room, and registration is refused only when no client
can be removed. An authorization a signed-in user is looking at keeps its
client.

Discovery and endpoints:

| Endpoint | Purpose |
|---|---|
| `/.well-known/oauth-protected-resource/mcp` | Resource metadata (RFC 9728; the 401 challenge names it, and the root well-known path is not served since it would describe a different resource) |
| `/.well-known/oauth-authorization-server` | OAuth server metadata |
| `POST /oauth/register` | Automatic public client registration |
| `GET /oauth/authorize` | Begin browser authorization |
| `GET/POST /oauth/consent` | Login continuation and explicit approval |
| `POST /oauth/token` | Code exchange and refresh |
| `POST /oauth/revoke` | Revoke a grant |

The OAuth server is implemented separately from MCP. It serves two resources:
`/mcp` and the [REST API](./api.md) (`/api/v1`, with its own scopes). A grant
is bound to one resource, so a token for one never opens the other.

## Transport details

- Supports protocol versions `2025-11-25`, `2025-06-18`, and `2025-03-26`.
- POST one JSON-RPC message to `/mcp`, with `Content-Type: application/json`,
  `Accept: application/json, text/event-stream`, and `Authorization: Bearer ...`.
- Send the negotiated `MCP-Protocol-Version` on subsequent requests.
- Responses use JSON. Notifications receive an empty HTTP 202 response.
- Stateless: no `MCP-Session-Id`, server-initiated requests, or background SSE
  stream. Authenticated GET and DELETE return 405, as permitted by Streamable HTTP.
- Request bodies are bounded to 64 KiB (16 KiB for OAuth endpoints).
- No new runtime dependencies or MCP SDK. Stdio and the older HTTP+SSE transport
  are not included.

## Tests

```bash
cargo test web::tests::oauth
cargo test web::tests::mcp
cargo test web::tests::invites
cargo test i18n::
```

The HTTP integration tests cover discovery, consent/login, PKCE and replay,
refresh rotation/concurrency, revocation, expiry, user and scope isolation,
protocol errors, pagination, timezone-aware busy intervals, database failures,
and generated links opening the existing booking flow.
