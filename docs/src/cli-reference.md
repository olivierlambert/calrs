# CLI Reference

## Global options

```
--data-dir <PATH>    Custom data directory (env: CALRS_DATA_DIR)
```

## Commands

### `calrs source`

Manage CalDAV calendar sources.

```
calrs source add [OPTIONS]
    --url <URL>           CalDAV server URL
    --username <USERNAME> CalDAV username
    --name <NAME>         Display name for this source
    --no-test             Skip the connection test

calrs source list

calrs source test <ID>    Test a connection (ID prefix match)

calrs source remove <ID>  Remove a source and all its data (ID prefix match)
```

### `calrs sync`

Pull latest events from all CalDAV sources.

```
calrs sync [OPTIONS]
    --full    Full re-sync (ignore sync tokens)
```

### `calrs calendar`

View synced calendar events.

```
calrs calendar show [OPTIONS]
    --from <DATE>    Start date (YYYY-MM-DD)
    --to <DATE>      End date (YYYY-MM-DD)
```

### `calrs event-type`

Manage bookable event types.

```
calrs event-type create [OPTIONS]
    --title <TITLE>              Event type title (required)
    --slug <SLUG>                URL slug (required)
    --duration <MINUTES>         Duration in minutes (required)
    --description <DESC>         Description
    --buffer-before <MINUTES>    Buffer before (default: 0)
    --buffer-after <MINUTES>     Buffer after (default: 0)

calrs event-type list

calrs event-type slots <SLUG> [OPTIONS]
    --days <DAYS>    Number of days to show (default: 7, clamped to the booking horizon)
```

`slots` consults any [shared resources](./resources.md) attached to the event type: slots where a required resource is busy are not printed, matching the web booking page.

If the event type sets a [booking horizon](./event-types.md#booking-horizon), `--days` is clamped to it, so the CLI shows the same window the booking page offers. Asking for `--days 30` on an event type with a 5-day horizon prints 6 days (today plus five).

### `calrs booking`

Manage bookings.

```
calrs booking create <SLUG> [OPTIONS]
    --date <DATE>          Booking date (YYYY-MM-DD)
    --time <TIME>          Start time (HH:MM)
    --name <NAME>          Guest name
    --email <EMAIL>        Guest email
    --timezone <TZ>        Guest timezone (default: UTC)
    --notes <NOTES>        Optional notes

calrs booking list [OPTIONS]
    --upcoming    Show only upcoming bookings

calrs booking cancel <ID>    Cancel a booking (ID prefix match)
```

`cancel` marks the booking cancelled, sends the cancellation emails, deletes the event from the host's CalDAV write-back calendar, and releases any [shared resource](./resources.md) reservation from the resource's CalDAV calendar(s).

### `calrs config`

Configure SMTP, authentication, and OIDC.

```
calrs config smtp [OPTIONS]
    --host <HOST>           SMTP server hostname
    --port <PORT>           SMTP port (default: 587)
    --username <USERNAME>   SMTP username (omit for an unauthenticated relay)
    --from-email <EMAIL>    Sender email address
    --from-name <NAME>      Sender display name
    --tls-mode <MODE>       starttls (default), tls, or none

calrs config show           Display current configuration

calrs config smtp-test <EMAIL>   Send a test email

calrs config auth [OPTIONS]
    --registration <BOOL>        Enable/disable registration
    --allowed-domains <DOMAINS>  Comma-separated domains or "any"

calrs config oidc [OPTIONS]
    --issuer-url <URL>        OIDC issuer URL
    --client-id <ID>          Client ID
    --client-secret <SECRET>  Client secret
    --enabled <BOOL>          Enable/disable OIDC
    --auto-register <BOOL>    Auto-create users on first login
```

#### Relaying through a local MTA

Leaving the username empty configures an unauthenticated relay, which is how you
send through a local MTA (Postfix, OpenSMTPD, Stalwart, Mailpit). calrs then
attaches no credentials at all, rather than authenticating with an empty
username, which such a relay rejects because it advertises no AUTH mechanism.

The TLS mode prompt accepts three values:

| Mode | Transport |
|---|---|
| `starttls` (default) | Plain connection upgraded with STARTTLS, typically port 587 |
| `tls` | Implicit TLS from the first byte, typically port 465 |
| `none` | No encryption, typically port 25 |

Use `none` only for a relay on the same machine. calrs validates certificates
against a compiled-in Mozilla root bundle rather than the system trust store, so
a relay presenting a self-signed or private-CA certificate (Debian's Postfix
default, for one) cannot be reached with `starttls` or `tls` either, and `none`
over the loopback is the way through. Mail sent this way, and any credentials
sent with it, cross the connection in the clear; calrs logs a warning if you
combine `none` with a username, or point it at a host that is not loopback.

The same applies to the `CALRS_SMTP_*` environment block, where only
`CALRS_SMTP_HOST` and `CALRS_SMTP_FROM_EMAIL` are required. A full local-relay
configuration is:

```
CALRS_SMTP_HOST=localhost
CALRS_SMTP_PORT=25
CALRS_SMTP_TLS_MODE=none
CALRS_SMTP_FROM_EMAIL=noreply@example.com
```

If the environment block sets no `CALRS_SMTP_USERNAME` while the database holds
SMTP credentials, the environment still wins and calrs relays unauthenticated.
It logs a warning once at startup when that happens, because the admin form
locks itself whenever the environment governs.

The two credential variables are not interchangeable when only one is present.
`CALRS_SMTP_PASSWORD` without `CALRS_SMTP_USERNAME` is an error: the password
can never be sent, so the configuration contradicts itself. A username without a
password is accepted and only warns, since a permissive relay may still
authenticate on the username alone, though in practice it usually means the
password never reached the process.

The equivalent from the CLI, with no prompts:

```
calrs config smtp --host localhost --port 25 --tls-mode none \
  --username '' --from-email noreply@example.com --from-name calrs
```

### `calrs resource`

Probe resource calendar URLs before adding them as [shared resources](./resources.md).

```
calrs resource probe [OPTIONS]
    --url <URL>           Resource calendar URL (ICS publish feed or CalDAV collection)
    --username <USERNAME> Username for authenticated CalDAV access (password prompted)
    --write-test          Write test: PUT a temporary event, verify it exists, then delete it
```

For CalDAV URLs the probe runs the full RFC 4791 discovery fallback. The write test confirms that reservation write-back will work with the given credentials.

### `calrs user`

Manage users (admin operations).

```
calrs user create [OPTIONS]
    --email <EMAIL>    User email
    --name <NAME>      User display name
    --admin            Grant admin role

calrs user list

calrs user set-password <EMAIL>

calrs user promote <EMAIL>     Promote to admin

calrs user demote <EMAIL>      Demote to regular user

calrs user disable <EMAIL>     Disable user account

calrs user enable <EMAIL>      Enable user account
```

### `calrs serve`

Start the web server.

```
calrs serve [OPTIONS]
    --port <PORT>    Port to listen on (default: 3000)
```
