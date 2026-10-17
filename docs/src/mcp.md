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
- Internal event types, which any authenticated colleague can distribute.
- Global administrators can manage all event types.

Each item has `id`, `slug`, `title`, `duration_min`, `visibility`, and `team_id`.
Use the **ID**, not the slug, when creating an invite. Slugs can be shared by
different users or teams. Follow `next_cursor` until it is `null`; permission
filtering can produce a short or empty page that still has a next cursor.

### `create_invite_link`

Arguments:

```json
{"event_type_id":"<id from list_event_types>"}
```

Example result:

```json
{
  "url": "https://cal.example/u/alice/intro?invite=<token>",
  "expires_at": "2026-10-23T12:00:00Z",
  "max_uses": 1
}
```

For internal and private events, the link expires after **seven days** and can be used for **one booking**. Guests
enter their own name and email. Team links use `/team/{team}/{event}?invite=...`.
The tool creates the same invitation records as the dashboard quick-link action;
it sends no email and creates no booking. You can manage the resulting internal
and private invites on the dashboard's invite-management page.

Internal events are available to any authenticated user. Private and public
events require owner, team-admin, or global-admin permissions. Disabled events
are rejected. Public events return their ordinary **reusable booking URL**, with
`expires_at: null` and `max_uses: null`, without inserting an invitation record.
For public events belonging to a private team, the URL includes that team's
existing reusable access token. Generating a link does not change visibility.

Each call for an internal/private event creates a **new** link. Avoid automatically
repeating a successful call. Link requests are limited to ten attempts per user
per five minutes.

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
an exactly registered HTTPS or loopback HTTP callback URI.

Access tokens last at most **one hour**. Refresh tokens rotate on each exchange
and expire **30 days after approval**; reconnect then. Clients must serialize
refreshes and save the replacement token: replay of a spent refresh token revokes
the whole grant. Tokens and codes are stored as SHA-256 hashes. Disabled users
lose access immediately, and permissions are checked again on every tool call.
Admin impersonation does not change the identity granting OAuth access.

Clients can disconnect using `POST /oauth/revoke` with form fields `client_id`
and `token` (access or refresh token). This revokes the whole grant. Logging out
of the browser does not revoke an integration's independent grant.

Discovery and endpoints:

| Endpoint | Purpose |
|---|---|
| `/.well-known/oauth-protected-resource/mcp` | Resource metadata (also available at the root well-known path) |
| `/.well-known/oauth-authorization-server` | OAuth server metadata |
| `POST /oauth/register` | Automatic public client registration |
| `GET /oauth/authorize` | Begin browser authorization |
| `GET/POST /oauth/consent` | Login continuation and explicit approval |
| `POST /oauth/token` | Code exchange and refresh |
| `POST /oauth/revoke` | Revoke a grant |

The OAuth server is implemented separately from MCP, so its grant/token machinery
can be extended to other first-party resources later. Currently `/mcp` is the
only supported resource.

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
