# CLI Reference

## Global options

```
--data-dir <PATH>    Custom data directory (env: CALRS_DATA_DIR)
```

## Commands

### `calrs init`

Interactive first-time setup. Creates an account with name, email, and timezone.

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
    --days <DAYS>    Number of days to show (default: 7)
```

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

### `calrs config`

Configure SMTP, authentication, and OIDC.

```
calrs config smtp [OPTIONS]
    --host <HOST>           SMTP server hostname
    --port <PORT>           SMTP port (default: 587)
    --username <USERNAME>   SMTP username
    --from-email <EMAIL>    Sender email address
    --from-name <NAME>      Sender display name

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
