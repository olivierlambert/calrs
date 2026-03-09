# CalDAV Integration

calrs connects to any CalDAV server to read your calendar for free/busy computation and optionally write confirmed bookings back.

## Connecting a calendar source

### From the web dashboard

1. Go to **Dashboard > Calendar sources > + Add**
2. Select your provider (BlueMind, Nextcloud, Fastmail, etc.) — the URL is auto-filled
3. Enter your username and password
4. Click **Add source**

The connection is tested automatically before saving. Use "Skip connection test" if your server doesn't respond to OPTIONS requests (e.g., BlueMind).

### From the CLI

```bash
calrs source add --url https://nextcloud.example.com/remote.php/dav \
                 --username alice --name "Work Calendar"

# Skip connection test if needed
calrs source add --url https://mail.company.com/dav/ \
                 --username alice --name "BlueMind" --no-test
```

## Provider URLs

| Provider | CalDAV URL |
|---|---|
| BlueMind | `https://mail.yourcompany.com/dav/` |
| Nextcloud | `https://cloud.example.com/remote.php/dav` |
| Fastmail | `https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/` |
| iCloud | `https://caldav.icloud.com/` |
| Google | `https://apidata.googleusercontent.com/caldav/v2/your@gmail.com/` |
| Zimbra | `https://mail.example.com/dav/` |
| SOGo | `https://mail.example.com/SOGo/dav/` |
| Radicale | `https://cal.example.com/` |

> **Tip:** Use app-specific passwords for Fastmail, iCloud, and Google.

## Auto-discovery

calrs follows the CalDAV standard (RFC 4791) for discovery:

1. **PROPFIND** on the base URL to find the `current-user-principal`
2. **PROPFIND** on the principal to find the `calendar-home-set`
3. **PROPFIND** on the calendar home to list all calendars
4. Filters to actual `calendar` collections (skips inbox, outbox, tasks, etc.)

## Syncing

```bash
# Sync all sources
calrs sync

# Full re-sync (ignore sync tokens)
calrs sync --full
```

From the dashboard, click **Sync** on any source to trigger a sync.

Sync pulls all VEVENT data from your calendars and stores it in the local SQLite database. Events are upserted by UID, so re-syncing is safe.

## CalDAV write-back

When a booking is confirmed, calrs can automatically push it to your CalDAV calendar as a VEVENT. When a booking is cancelled, the event is deleted.

### Setup

1. Sync your calendar source at least once (so calrs knows which calendars exist)
2. On the dashboard, find your source under "Calendar sources"
3. Use the **"Write bookings to"** dropdown to select which calendar should receive bookings
4. Select "None" to disable write-back

### How it works

- On **confirmation**: calrs generates an ICS event and PUTs it to `{calendar-href}/{booking-uid}.ics`
- On **cancellation**: calrs DELETEs the event from the same path
- The booking tracks which calendar it was pushed to, so cancellation always targets the right calendar
- If no write calendar is configured, write-back is silently skipped (emails still work)
- Write-back works for individual bookings, group round-robin bookings, and pending-then-confirmed bookings

## Managing sources

```bash
# List all sources
calrs source list

# Test a connection
calrs source test <id-prefix>

# Remove a source (cascade-deletes calendars and events)
calrs source remove <id-prefix>
```

From the dashboard: **Sync**, **Test**, and **Remove** buttons are available for each source.

## Credentials

Passwords are hex-encoded and stored in the SQLite database. This is not encryption — it prevents accidental display in logs but does not protect against database access. Secure your data directory appropriately.
