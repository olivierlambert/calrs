# Getting Started

## Install and start

See [Deployment](./deployment.md) for containers, systemd and source builds. To try the v2 prerelease channel with an isolated data volume:

```bash
docker run -d --name calrs-v2 \
  -p 127.0.0.1:3000:3000 \
  -v calrs-v2-data:/var/lib/calrs \
  -e CALRS_BASE_URL=http://localhost:3000 \
  ghcr.io/olivierlambert/calrs:next
```

`next` follows prereleases; `latest` follows stable releases. Pin an exact available version for a repeatable installation. Use HTTPS and the real public base URL when deploying for guests or configuring calendar OAuth and SSO. See [Upgrading to v2](./upgrading.md) before reusing an existing data volume.

For a local source checkout:

```bash
cargo build --release --locked
./target/release/calrs serve --port 3000
```

Run from the checkout root so calrs finds `templates/`.

## Set up in the dashboard

1. Open `http://localhost:3000` and register. The first registered user becomes admin.
2. In **Profile & Settings**, check your username, timezone, language and default working hours.
3. In **Calendar sources**, connect your provider. Use [CalDAV](./caldav.md), [Google Calendar](./google-calendar.md), [Microsoft 365 / Outlook](./microsoft-calendar.md), or [Exchange EWS](./exchange.md). Google and Microsoft require an administrator to configure an OAuth application first. Your administrator may instead provision a managed Exchange source.
4. Sync the source and check the result. Choose which calendars to sync and a writable calendar for confirmed bookings. These are separate choices: a calendar can receive bookings without being read for availability.
5. Create an **Event type**. Set its title, slug, duration, timezone, weekly hours and location. Decide whether requests need approval, how far ahead guests can book, and whether to ask [booking questions](./booking-questions.md).
6. Configure and test [SMTP](./admin.md#smtp-configuration) if you want emails. Without SMTP, bookings still work and confirmed bookings have an `.ics` download.
7. Open your booking link in another browser or a private window. Make a test booking, check the dashboard and calendar, then cancel it to verify cleanup.

Your profile is at `/u/yourname`, and an event type with slug `intro` is at `/u/yourname/intro`. Only public, enabled and still-bookable event types appear for anonymous visitors. Internal and private event types use [invite links](./event-types.md#visibility).

## CLI setup for a single-user instance

The web dashboard is the recommended setup path on multi-user instances. Several scheduling CLI commands select the first account rather than a signed-in user; they are operator tools with direct database access.

```bash
# Create the first user; the password is prompted
calrs user create --email alice@example.com --name Alice --admin

calrs source add --url https://nextcloud.example.com/remote.php/dav \
  --username alice --name 'My Calendar'
calrs sync
calrs event-type create --title '30-minute intro call' --slug intro --duration 30
calrs event-type slots intro
calrs serve --port 3000
```

Use the same `--data-dir` or `CALRS_DATA_DIR` for CLI commands and the server. There is no `calrs init` command in v2. See [CLI Reference](./cli-reference.md) for password input in scripts, source selection and configuration commands.

## Data directory

calrs stores its database and credential key in the data directory. Defaults follow platform conventions:

- Linux: `~/.local/share/calrs/`
- macOS: `~/Library/Application Support/calrs/`
- Project container and systemd service: `/var/lib/calrs`

Override with `CALRS_DATA_DIR` or `--data-dir`. Back up the database, `secret.key` and uploaded files together; see [Backup](./deployment.md#backup).
