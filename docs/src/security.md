# Security

This page documents calrs's security measures and known limitations.

## Authentication

- **Password hashing** — Argon2 with random salt (via the `argon2` + `password-hash` crates). Passwords are never stored in plaintext.
- **Sessions** — 32-byte random tokens (cryptographically secure via `OsRng`), stored server-side in SQLite with 30-day TTL.
- **Cookie flags** — All session cookies use `HttpOnly; Secure; SameSite=Lax`. The `Secure` flag ensures cookies are only sent over HTTPS.
- **OIDC** — Authorization code flow with PKCE, state validation, and nonce verification. Tested with Keycloak.

## Rate limiting

Login attempts are rate-limited per IP address:

- **10 attempts** per **15-minute window**
- After the limit, further attempts return an error without checking credentials
- The client IP is read from the `X-Forwarded-For` header (set by your reverse proxy)

> **Important:** Make sure your reverse proxy sets `X-Forwarded-For` correctly. Without it, rate limiting falls back to a single "unknown" bucket and won't be effective.

### Nginx

```nginx
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
```

### Caddy

Caddy sets `X-Forwarded-For` automatically.

### Booking endpoints

Booking submissions are rate-limited per IP address:

- **10 attempts** per **5-minute window**
- Applies to all 4 booking handlers (user, group, team link, legacy)

## CSRF protection

All POST forms are protected against cross-site request forgery using the **double-submit cookie** pattern:

- A `calrs_csrf` cookie is set automatically on every response (via middleware)
- Client-side JavaScript reads the cookie and injects a hidden `_csrf` field into all POST forms
- On submission, the server verifies that the cookie value matches the form field
- Mismatches return a `403 Forbidden` response

This protects all 31 POST endpoints including booking submissions, settings changes, admin actions, and authentication forms. Multipart forms (avatar/logo upload) pass the token via query parameter.

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

Additionally, all booking handlers wrap the availability check and INSERT in a database transaction (`BEGIN IMMEDIATE`), preventing race conditions between concurrent requests.

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

### No brute-force account lockout

Rate limiting is per-IP, not per-account. A distributed attack from many IPs would not be rate-limited. Consider using fail2ban or your reverse proxy's rate limiting for additional protection.

### SSRF (server-side request forgery)

CalDAV source URLs are user-supplied. A malicious user could point a CalDAV source at an internal IP (e.g., `http://127.0.0.1:8080/`) to probe internal services. In a trusted multi-user deployment (e.g., behind OIDC), this is low risk. For public-registration instances, consider restricting network access at the firewall level.

## Recommendations for production

1. **Always use HTTPS** — the `Secure` cookie flag requires it
2. **Set `CALRS_BASE_URL`** to your public HTTPS URL
3. **Configure your reverse proxy** to set `X-Forwarded-For` correctly
4. **Restrict filesystem access** to the data directory (contains the SQLite database with credentials)
5. **Disable registration** if using OIDC (`calrs config auth --registration false`)
6. **Keep calrs updated** for security patches
