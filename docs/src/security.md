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

## ICS injection protection

User-supplied values (guest name, email, event title, location, notes) are sanitized before being inserted into `.ics` calendar invites:

- Carriage returns (`\r`) and newlines (`\n`) are stripped to prevent ICS field injection
- Semicolons and commas are escaped per RFC 5545

This prevents attackers from injecting arbitrary iCalendar properties (e.g., extra attendees, recurrence rules) through booking form fields.

## SQL injection

All database queries use parameterized bindings via `sqlx`. No SQL is constructed through string concatenation.

## XSS (cross-site scripting)

All HTML output is rendered through Minijinja, which **auto-escapes** all template variables by default. No `|safe` or `|raw` filters are used.

## Known limitations

### No CSRF tokens

Forms do not include CSRF tokens. The `SameSite=Lax` cookie attribute provides partial protection (blocks cross-site POST submissions from iframes/AJAX), but does not protect against top-level form submissions from malicious pages.

**Mitigation:** If your instance is behind an SSO provider (OIDC), the attack surface is reduced since an attacker would need the user to be logged in.

### CalDAV credential storage

CalDAV passwords are stored as hex-encoded strings in SQLite. This prevents accidental display in logs but is **not encryption** — anyone with access to the database file can decode them. Secure your data directory with filesystem permissions.

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
