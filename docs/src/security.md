# Security

This page documents calrs's security measures and known limitations.

## Authentication

- **Password hashing** — Argon2 with random salt (via the `argon2` + `password-hash` crates). Passwords are never stored in plaintext.
- **Sessions** — 32-byte random tokens (cryptographically secure via `OsRng`), stored server-side in SQLite with 30-day TTL.
- **Cookie flags** — All session cookies use `HttpOnly; Secure; SameSite=Lax`. The `Secure` flag ensures cookies are only sent over HTTPS.
- **OIDC** — Authorization code flow with PKCE, state validation, and nonce verification. Tested with Keycloak.
- **Local MFA** — Optional TOTP, enforceable by the administrator for local accounts. A ten-minute challenge grants no session access until verification or required enrollment succeeds. Secrets are encrypted with AES-256-GCM; recovery codes contain 128 random bits and are stored as SHA-256 hashes. Code consumption and session creation are transactional to prevent replay under concurrent requests. Setup and recovery pages use `Cache-Control: no-store`.

MFA code verification and management also have a persistent account-wide limit
of ten attempts per 15 minutes. TOTP accepts the current 30-second interval and
one neighboring interval in either direction. Previously accepted intervals
cannot be reused. TOTP protects against a stolen password alone; it is not
phishing-resistant. See [MFA setup and recovery](authentication.md#two-factor-authentication-totp).

## Rate limiting

Login attempts are rate-limited per IP address:

- **10 attempts** per **15-minute window**
- After the limit, further attempts return an error without checking credentials
- The client address is the TCP peer, or, when the peer is a trusted proxy, the address that proxy reports in `X-Forwarded-For` (see below)

The same rule identifies the client for every per-address limit (sign-in, two-factor codes, bookings, OAuth). An IPv6 client counts by its /64, so rotating addresses inside it does not buy a new budget.

### Trusted proxies

calrs believes `X-Forwarded-For` only from a **trusted proxy**. It reads the header right to left, skipping the hops that are themselves trusted proxies; the first address that is not one is the client. A peer that is not trusted is the client itself, and whatever `X-Forwarded-For` it sends is ignored, so a client cannot claim a fresh address on every attempt.

The trusted list is, in order of precedence:

1. `CALRS_TRUSTED_PROXIES`: a comma or space separated list of addresses and CIDR ranges (`127.0.0.1, 10.0.0.0/8, fd00::/8`), or `none` to trust no peer. An unusable value trusts no peer and logs an error.
2. **Trusted proxies** in the admin panel (System settings), same format. Empty means the default.
3. The default: loopback (`127.0.0.0/8`, `::1`), private (`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`), unique local (`fc00::/7`) and link-local (`169.254.0.0/16`, `fe80::/10`).

What this means for common setups:

- **Caddy or Nginx on the same host, or on the same Docker network**: works out of the box, the proxy's address is in the default list. Make sure it sets `X-Forwarded-For` (Caddy does; Nginx needs the line below).
- **Docker's userland proxy** (ports published over IPv6, or hairpin connections from the host): every client arrives from the bridge gateway, a trusted address that sends no header, so all clients share one rate-limit bucket and one person mistyping a password can lock everyone out for 15 minutes. Put the reverse proxy in front, or publish the port with the userland proxy disabled so calrs sees real client addresses.
- **calrs exposed directly on a LAN, no proxy**: by default LAN peers are trusted, so a LAN client could forge `X-Forwarded-For`. Set `CALRS_TRUSTED_PROXIES=none`, or list only your proxy's address.
- **calrs exposed directly on the internet**: public peers are never trusted by default, so each client is counted by its own address.
- **A proxy on a public address** (a load balancer, a CDN): list its addresses or ranges, otherwise every client is counted as the proxy.

### Nginx

```nginx
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
```

### Caddy

Caddy sets `X-Forwarded-For` automatically.

### Booking endpoints

Booking submissions are rate-limited per IP address:

- **10 attempts** per **5-minute window**
- Applies to all booking handlers (user, team, and legacy)
- The client IP is the rightmost `X-Forwarded-For` value, the address your reverse proxy saw
- The counters are stored in the database (`rate_limit_counters`), so restarting calrs does not reset them; closed windows are pruned hourly

Login and the other limits are kept in memory and start over when calrs restarts.

## CSRF protection

All POST forms are protected against cross-site request forgery using the **double-submit cookie** pattern:

- A `calrs_csrf` cookie is set automatically on every response (via middleware)
- Client-side JavaScript reads the cookie and injects a hidden `_csrf` field into all POST forms
- On submission, the server verifies that the cookie value matches the form field
- Mismatches return a `403 Forbidden` response

This protects all 31 POST endpoints including booking submissions, settings changes, admin actions, and authentication forms. Multipart forms (avatar/logo/favicon upload) pass the token via query parameter.

The cookie uses `SameSite=Lax` and is intentionally NOT `HttpOnly` so the client-side script can read it.

## Input validation

All user-submitted data is validated server-side:

- **Booking forms** — name (1–255 chars), email (format + length), notes (max 5,000 chars), date (max 365 days in the future)
- **Registration** — name (1–255 chars), email format and length validation
- **Settings** — name length, booking email format validation
- **Avatar upload** — strict content-type whitelist (JPEG, PNG, GIF, WebP only)
- **HTML templates** — `maxlength` attributes on form inputs (defense in depth)

## ICS injection protection

User-supplied values (guest name, email, event title, location, notes) are sanitized before being inserted into `.ics` calendar invites:

- Carriage returns (`\r`) and newlines (`\n`) are stripped to prevent ICS field injection
- Semicolons and commas are escaped per RFC 5545

This prevents attackers from injecting arbitrary iCalendar properties (e.g., extra attendees, recurrence rules) through booking form fields.

## SQL injection

All database queries use parameterized bindings via `sqlx`. No SQL is constructed through string concatenation.

## XSS (cross-site scripting)

All HTML output is rendered through Minijinja, which **auto-escapes** all template variables by default. No `|safe` or `|raw` filters are used.

## Double-booking prevention

A SQLite partial unique index prevents two bookings for the same event type and time slot:

```sql
CREATE UNIQUE INDEX idx_bookings_no_overlap
ON bookings(event_type_id, start_at)
WHERE status IN ('confirmed', 'pending');
```

Every new booking, guest reschedule and approval also runs its availability checks and the write that claims the time under one process-wide lock (`resources::booking_lock()`), so concurrent requests are checked one after the other and each sees the bookings committed before it, including bookings of the host's other event types, which the index does not cover. The database transaction only wraps the writes and is opened after the checks, so no request waits for a pool connection while holding one. Nothing under the lock touches the network: resource feeds and calendars are refreshed before it is taken, and the checks under it read cached data. The lock lives in the server process: `calrs booking create`, run from another process, relies on the unique index and the database checks only.

At booking time an availability read that fails (calendar events, bookings, time off, working hours, shared resources) refuses the request with an internal error instead of treating the host as free. Slot pages degrade softly: they log the error and show what they could read.

## Error handling

Web handlers use explicit error handling instead of panics. Template rendering failures, date parsing errors, and database errors return user-friendly HTTP error responses rather than crashing the server process.

## Token-based actions

Certain actions can be performed without authentication, using single-use-like tokens:

- **Cancel token** — allows guests to cancel their booking via a link in the confirmation email
- **Confirm token** — allows hosts to approve or decline pending bookings via links in the approval request email

Tokens are UUID v4 (128-bit random), stored with unique indexes in the database. They are not invalidated after use (the booking status check prevents replay — a token for an already-confirmed booking shows "already approved"). These links should be treated as sensitive — anyone with the link can perform the action.

## Known limitations

### CalDAV credential storage

CalDAV and SMTP passwords are encrypted at rest using **AES-256-GCM**. The encryption key is auto-generated at `$DATA_DIR/secret.key` on first run, or can be provided via the `CALRS_SECRET_KEY` environment variable. Legacy hex-encoded passwords (from pre-v0.10.0) are auto-migrated to encrypted format on startup. Protect your `secret.key` file with filesystem permissions.

In memory, the key and the decrypted credentials calrs holds are kept in buffers that are wiped when released (`zeroize`): the key and its long-lived copies, the plaintext returned by every decryption, the CalDAV, EWS and SMTP passwords and SMS gateway secrets in calrs's own configuration and client structures, OAuth access tokens, and the captcha, meeting webhook and EWS service-account secrets the server keeps for its whole life; the AES key schedule is wiped too. This narrows what a core dump or a later memory disclosure could reveal. It does not reach the copies that libraries make while using a credential (the HTTP client's request headers, the SMTP client's credentials, the OpenID Connect client secret) or the `CALRS_SECRET_KEY` value in the process environment, which the operating system keeps for the life of the process.

### No brute-force account lockout

Rate limiting is per-IP, not per-account. A distributed attack from many IPs would not be rate-limited. Consider using fail2ban or your reverse proxy's rate limiting for additional protection.

### SSRF (server-side request forgery)

CalDAV source URLs are user-supplied. `validate_caldav_url()` blocks URLs whose hostname resolves to a private or reserved IP range (loopback, RFC1918, link-local, ULA, etc.) before any HTTP request is issued.

This check resolves the hostname once at validation time, so a DNS-rebinding attacker who answers the initial lookup with a public IP and a subsequent lookup (during the actual HTTP fetch) with a private IP can bypass the guard. Mitigating this purely at the application layer would require inspecting the connected socket's peer address after every TCP handshake, which is outside the scope of the current implementation. The recommended deployment posture is therefore an **egress firewall** that prevents calrs from reaching RFC1918 / link-local ranges regardless of what DNS returns.

#### Allowing private CalDAV hosts (self-hosting)

Self-hosted deployments often run calrs and the CalDAV server on the same private network (e.g. a docker-compose stack where `http://radicale:5232` resolves to an RFC1918 address). To permit specific hostnames to resolve to private/reserved IPs, configure a comma-separated allowlist of hostnames (or literal IPs). Two ways, in order of precedence:

1. **Environment variable** (takes precedence — ops override):
   ```
   CALRS_ALLOW_PRIVATE_HOSTS=radicale,nextcloud.local,127.0.0.1
   ```
2. **Admin UI / CLI** (persisted in the DB, used when the env var is unset):
   - Admin panel → **System settings** → *Private-host allowlist*
   - `calrs config general --allow-private-hosts radicale,nextcloud.local,127.0.0.1`

When the environment variable is set it overrides the stored value (the admin panel shows a "set by environment" badge), and neither the admin panel nor `calrs config general` will write that field to the database — they refuse and say so. That is deliberate: a value stored while the env var was set would sit dormant and then silently become the effective allowlist the day the variable is removed. Change it where it is set, or unset the variable first. Matching is case-insensitive and exact (no wildcards or subdomain matching). Only the listed hosts bypass the private-IP check; every other host is still validated. Keep this list as small as possible, scheme validation (http/https only) still applies.

In a trusted multi-user deployment (e.g., behind OIDC) this is low risk. For public-registration instances, configure egress filtering at the network level.

## Recommendations for production

1. **Always use HTTPS** — the `Secure` cookie flag requires it
2. **Set `CALRS_BASE_URL`** to your public HTTPS URL
3. **Configure your reverse proxy** to set `X-Forwarded-For` correctly
4. **Restrict filesystem access** to the data directory (contains the SQLite database with credentials)
5. **Disable registration** if using OIDC (`calrs config auth --registration false`)
6. **Keep calrs updated** for security patches
