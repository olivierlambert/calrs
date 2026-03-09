# calrs

**Fast, self-hostable scheduling. Like Cal.com, but written in Rust.**

> _"Your time, your stack."_

## What is this?

`calrs` is an open-source scheduling platform built in Rust. Connect your CalDAV calendar (Nextcloud, Fastmail, BlueMind, iCloud, Google…), define bookable meeting types, and share a link. No Node.js, no PostgreSQL, no subscription.

## Status

Early development. CLI and web booking page are functional.

## Quick start

```bash
# Build
cargo build --release

# Initialize
calrs init

# Connect your CalDAV calendar (see "Connecting your calendar" below)
calrs source add --url https://nextcloud.example.com/remote.php/dav \
                 --username alice --name "My Calendar"

# Pull events
calrs sync

# Create a bookable meeting type
calrs event-type create --title "30min intro call" --slug intro --duration 30

# Check your availability
calrs event-type slots intro

# Book a slot
calrs booking create intro --date 2026-03-20 --time 14:00 \
  --name "Jane Doe" --email jane@example.com

# See your upcoming events
calrs calendar show

# List bookings
calrs booking list --upcoming

# Start the web booking page
calrs serve --port 3000
# Then visit http://localhost:3000/intro
```

## Connecting your calendar

calrs connects to any CalDAV server. You need the **DAV root URL** for your provider — not a calendar-specific or public link.

### Common CalDAV URLs

| Provider | URL |
|---|---|
| **Nextcloud** | `https://your-server.com/remote.php/dav` |
| **BlueMind** | `https://your-server.com/dav/` |
| **Fastmail** | `https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/` |
| **iCloud** | `https://caldav.icloud.com/` |
| **Google** | `https://apidata.googleusercontent.com/caldav/v2/your@gmail.com/` |
| **Zimbra** | `https://your-server.com/dav/` |
| **SOGo** | `https://your-server.com/SOGo/dav/` |
| **Radicale** | `https://your-server.com/` |

### Example

```bash
# Nextcloud
calrs source add --url https://cloud.example.com/remote.php/dav \
                 --username alice --name Nextcloud

# BlueMind (use --no-test if OPTIONS hangs)
calrs source add --url https://mail.example.com/dav/ \
                 --username alice --name BlueMind --no-test

# Fastmail
calrs source add --url https://caldav.fastmail.com/dav/calendars/user/alice@fastmail.com/ \
                 --username alice@fastmail.com --name Fastmail
```

calrs will auto-discover your principal URL and calendar-home-set via PROPFIND (RFC 4791). If the connection test hangs or fails, use `--no-test` to skip it and go straight to `calrs sync`.

## CLI reference

```
calrs init                           First-time setup
calrs source add [--no-test]         Connect a CalDAV calendar
calrs source list                    List connected sources
calrs source remove <id>             Remove a source
calrs source test <id>               Test a connection
calrs sync [--full]                  Pull latest events from CalDAV
calrs event-type create              Define a new bookable meeting
calrs event-type list                List your event types
calrs event-type slots <slug>        Show available slots
calrs calendar show [--from] [--to]  View your calendar
calrs booking create <slug>          Book a slot
calrs booking list [--upcoming]      View bookings
calrs booking cancel <id>            Cancel a booking
calrs config smtp                    Configure SMTP for email notifications
calrs config show                    Show current configuration
calrs config smtp-test <email>       Send a test email
calrs config auth                    Configure registration/domain restrictions
calrs config oidc                    Configure OIDC (SSO via Keycloak, etc.)
calrs user list                      List users
calrs user create                    Create a user
calrs user set-password <email>      Set a user's password
calrs user promote <email>           Promote user to admin
calrs serve [--port 3000]            Start the web booking server
```

## Architecture

```
calrs/
├── Cargo.toml
├── CLAUDE.md
├── README.md
├── migrations/
│   └── 001_initial.sql        SQLite schema
├── templates/
│   ├── base.html              Base layout + CSS
│   ├── auth/
│   │   ├── login.html         Login page (local + SSO)
│   │   └── register.html      Registration page
│   ├── dashboard.html         User dashboard
│   ├── event_type_form.html   Create/edit event types
│   ├── profile.html           Public user profile
│   ├── slots.html             Available time slots
│   ├── book.html              Booking form
│   └── confirmed.html         Confirmation page
└── src/
    ├── main.rs                CLI entry point (clap)
    ├── db.rs                  SQLite connection + migrations
    ├── models.rs              Domain types
    ├── auth.rs                Authentication (local + OIDC)
    ├── email.rs               SMTP email with .ics invites
    ├── caldav/
    │   └── mod.rs             CalDAV client (RFC 4791)
    ├── web/
    │   └── mod.rs             Axum web server + booking handlers
    └── commands/
        ├── mod.rs             Re-exports
        ├── init.rs            calrs init
        ├── source.rs          calrs source add/list/remove/test
        ├── sync.rs            calrs sync
        ├── calendar.rs        calrs calendar show
        ├── event_type.rs      calrs event-type create/list/slots
        ├── booking.rs         calrs booking create/list/cancel
        ├── config.rs          calrs config smtp/show/smtp-test/auth/oidc
        └── user.rs            calrs user create/list/promote/demote
```

**Storage:** SQLite (WAL mode). Single file, zero ops.

**CalDAV:** Pull-based sync. Reads your existing calendars for free/busy.
Does not write to your CalDAV server (bookings are stored locally, with optional push).

## Authentication

calrs supports local accounts (email/password) and SSO via OpenID Connect (Keycloak, Authentik, etc.).

The first registered user automatically becomes admin.

### OIDC setup (Keycloak example)

1. In your Keycloak realm, create a new **OpenID Connect** client:
   - **Client ID**: `calrs`
   - **Client authentication**: ON (confidential)
   - **Valid redirect URIs**: `https://your-calrs-host/auth/oidc/callback`
   - **Web origins**: `https://your-calrs-host`

2. Copy the **Client secret** from the Credentials tab.

3. Configure calrs:

```bash
calrs config oidc \
  --issuer-url https://keycloak.example.com/realms/your-realm \
  --client-id calrs \
  --client-secret YOUR_CLIENT_SECRET \
  --enabled true \
  --auto-register true
```

4. Set the base URL and start:

```bash
export CALRS_BASE_URL=https://your-calrs-host
calrs serve --port 3000
```

The login page will show a "Sign in with SSO" button. With `--auto-register true`, users are created automatically on first OIDC login. Existing local users are linked by email.

### Registration control

```bash
# Disable open registration
calrs config auth --registration false

# Restrict to specific email domains
calrs config auth --allowed-domains "example.com,company.org"
```

## Roadmap

- [x] CalDAV sync (pull)
- [x] SQLite storage
- [x] CLI availability viewer
- [x] Booking engine with conflict detection
- [x] Email notifications (SMTP) with `.ics` calendar invite
- [x] Web booking page (Axum + minijinja, no JS framework)
- [x] Authentication (local + OIDC/SSO)
- [x] User management (admin/user roles)
- [ ] Group sync from OIDC provider
- [ ] CalDAV write (push confirmed bookings back to your calendar)
- [ ] Recurrence rule expansion
- [ ] Multi-timezone support
- [ ] Docker image

## License

AGPL-3.0 — free to use, modify, and self-host. Contributions welcome.
