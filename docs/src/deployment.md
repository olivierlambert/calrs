# Deployment

This guide covers v2, currently a prerelease. Read [Upgrading to v2](./upgrading.md) before using an existing data directory. The examples use `next`; choose an exact tag from [Releases](https://github.com/olivierlambert/calrs/releases) for a repeatable deployment.

## Docker / Podman (recommended)

Pre-built images are available on [GitHub Container Registry](https://github.com/olivierlambert/calrs/pkgs/container/calrs) for `amd64` and `arm64`:

```bash
docker run -d --name calrs \
  -p 127.0.0.1:3000:3000 \
  -v calrs-data:/var/lib/calrs \
  -e CALRS_BASE_URL=https://cal.example.com \
  ghcr.io/olivierlambert/calrs:next
```

> **Podman** works as a drop-in replacement — just use `podman` instead of `docker` in all commands. The Containerfile (Dockerfile) is compatible with both runtimes.

`latest` follows stable releases; `next` follows prereleases. An exact prerelease tag looks like `ghcr.io/olivierlambert/calrs:2.0.0-beta.4`. The loopback port binding above assumes a reverse proxy on the host; adapt it if your proxy runs in a separate container.

The image uses a multi-stage build:

- **Builder:** `rust:slim-trixie` — compiles the release binary
- **Runtime:** `debian:trixie-slim` — minimal image with only `ca-certificates`
- Runs as unprivileged `calrs` user
- Data stored in `/var/lib/calrs`
- Templates bundled at `/opt/calrs/templates/`

To build from source instead: `docker build -t calrs .`

## Docker Compose / Podman Compose

```yaml
services:
  calrs:
    image: ghcr.io/olivierlambert/calrs:next
    ports:
      - "127.0.0.1:3000:3000"
    volumes:
      - calrs-data:/var/lib/calrs
    environment:
      - CALRS_BASE_URL=https://cal.example.com
    restart: unless-stopped

volumes:
  calrs-data:
```

Works with both `docker compose` and `podman-compose`.

## RPM (RHEL / Fedora)

Community-maintained RPM packages are published on Copr by [@wallon-ines](https://github.com/wallon-ines) for Enterprise Linux 9 and 10:

```bash
# Enable the repo (RHEL 9/10, Rocky, AlmaLinux, ...)
sudo dnf copr enable missd/calrs
sudo dnf install calrs

# Start the service
sudo systemctl enable --now calrs
```

- Copr repository: <https://copr.fedorainfracloud.org/coprs/missd/calrs/>
- RPM spec source: <https://gitlab.famillewallon.com/rpm-packages/calrs>

These packages are not maintained by the calrs project itself; for issues with the package (versioning, dependencies, systemd unit), reach out via the Copr/GitLab links above.

## Binary + systemd

```bash
# Build from source
cargo build --release

# Create the service account and data directory first (new installation)
sudo useradd -r -s /bin/false -m -d /var/lib/calrs calrs
sudo install -d -o calrs -g calrs -m 750 /var/lib/calrs

# Install the binary and matching templates
sudo install -m 755 target/release/calrs /usr/local/bin/calrs
sudo cp -r templates /var/lib/calrs/templates

# Install the service
sudo cp calrs.service /etc/systemd/system/
sudo systemctl daemon-reload
# Set the public URL before starting (see below)
sudo systemctl edit calrs
sudo systemctl enable --now calrs
```

In the service override opened by `systemctl edit`, set:

```ini
[Service]
Environment=CALRS_BASE_URL=https://cal.example.com
Environment=CALRS_TRUSTED_PROXIES=127.0.0.1,::1
```

The included unit runs from `/var/lib/calrs`, where it finds `templates/`, and listens on `127.0.0.1:3000`. Put your TLS reverse proxy on that host, or deliberately change the bind address and proxy trust for a different topology. Install matching templates whenever you replace the binary.

### systemd service

The included `calrs.service` has security hardening:

- `NoNewPrivileges=true`
- `ProtectSystem=strict`
- `ProtectHome=true`
- `ReadWritePaths=/var/lib/calrs`
- `PrivateTmp=true`
- `ProtectKernelTunables=true`
- `ProtectControlGroups=true`
- `Restart=on-failure` with 5-second delay

## From source (development)

```bash
cargo build --release --locked
./target/release/calrs serve --port 3000
```

Then register at `http://localhost:3000` — the first user becomes admin.

## Reverse proxy

calrs listens on port 3000 by default. Put nginx or caddy in front for TLS.

### nginx example

```nginx
server {
    listen 443 ssl http2;
    server_name cal.example.com;

    ssl_certificate /etc/letsencrypt/live/cal.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/cal.example.com/privkey.pem;

    # Room for the 2 MB logo and avatar uploads (nginx's default is 1 MB).
    client_max_body_size 3m;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Caddy example

```
cal.example.com {
    reverse_proxy localhost:3000
}
```

## Environment variables

| Variable | Description | Default |
|---|---|---|
| `CALRS_DATA_DIR` | Database, credential key and uploaded files directory | `/var/lib/calrs` (Docker/systemd) or XDG (dev) |
| `CALRS_BASE_URL` | Public URL (required for OIDC callbacks and email action links) | `http://localhost:3000` |
| `CALRS_ALLOW_PRIVATE_HOSTS` | Comma-separated hostnames or IP addresses allowed to resolve to private IPs for CalDAV/EWS (SSRF opt-out) | _(none)_ |
| `CALRS_TRUSTED_PROXIES` | Comma or space separated addresses and CIDR ranges of the reverse proxies whose `X-Forwarded-For` calrs believes for its rate limits, or `none` to trust no peer. A `/0` range is refused. See [Trusted proxies](./security.md#trusted-proxies) | loopback, private, unique local and link-local ranges |
| `CALRS_SYNC_MIN_AGE` | Minimum time in seconds between two background syncs of the same calendar source, e.g. `900` for every 15 minutes. Booking pages still refresh a host's calendars on demand once they are 5 minutes old, and a source due its daily full resync is synced anyway. Capped at `86400` (one day). A failing source is retried at this interval too, so longer intervals can delay the email warning its owner (sent after at least three failures spanning more than an hour) | `0` (no minimum: each source about once an hour, see below) |
| `CALRS_SYNC_CONCURRENCY` | How many calendar sources the background sync may sync at once, never more than one per server. Capped at `8`: syncs share the database's five connections with web requests | `4` |
| `CALRS_SECRET_KEY` | Optional external credential-encryption key: a base64-encoded 32-byte key. Preserve the same key across restarts and restores | Generated `secret.key` in the data directory |
| `CALRS_SMTP_*` | SMTP environment configuration; overrides stored settings. See [SMTP](./admin.md#smtp-configuration) | _(none)_ |
| `RUST_LOG` | Log level filter | `calrs=info,tower_http=info` |
| `CALRS_SMS_*` | SMS gateway configuration, overriding the admin panel; see [SMS Notifications](./sms.md#environment-variables) | _(none)_ |

`CALRS_BASE_URL`, `CALRS_ALLOW_PRIVATE_HOSTS` and `CALRS_TRUSTED_PROXIES` can also be set without environment variables, from the admin panel (**System settings**: base URL, private hosts, **Trusted proxies**); the first two also from the CLI (`calrs config general`). The stored value is persisted in the database; when the matching environment variable is set it takes precedence at runtime and the field is read-only. System settings check every field before saving any, so a refused value saves nothing from the form. The other variables (`CALRS_DATA_DIR`, `RUST_LOG`, and `CALRS_SECRET_KEY`) are bootstrap settings and remain environment-only.

### Background calendar sync

Besides the on-demand refresh of booking pages, `calrs serve` syncs calendar sources in the background. Once a minute it syncs enough sources for every one to come round about once an hour: one per minute up to 60 sources (as in earlier versions), then the number of sources divided by 60 (24 per minute for 1,440 sources). Sources due their daily full resync go first; then the ones synced longest ago. Up to `CALRS_SYNC_CONCURRENCY` syncs run at once, but never two against the same server (scheme, host and port), so one CalDAV or Exchange server sees one background sync at a time; [managed Exchange sources](./ews-impersonation.md) count against the server of the global Exchange endpoint. Microsoft 365 and Google calendars, which throttle each user separately, count each source on its own. A slow server holds one slot only: the other slots carry on with sources on other servers, and the next minute starts on time. A sync still running after 10 minutes is stopped and counts as a failure of its source. Reminders, request expiry and write-back retries run in a task of their own and never wait for any of this. A source being synced already is not synced twice: a booking page that finds its host's source being synced shows the cached times without waiting, and the **Sync** button waits up to 20 seconds for it, and an approval up to 10 seconds, so that it checks the calendar that sync fetched. The `calrs sync` CLI is a separate process and is not coordinated with a running server.

## Observability

calrs uses structured logging via the `tracing` crate. All log output goes to stderr, captured by systemd journal or Docker logs.

### Log levels

```bash
# Default (recommended)
RUST_LOG=calrs=info,tower_http=info

# Verbose (includes per-request details)
RUST_LOG=calrs=debug,tower_http=debug

# Errors only
RUST_LOG=calrs=error
```

### What's logged

| Category | Level | Events |
|----------|-------|--------|
| Auth | info/warn | Login success/failure, registration, logout, OIDC login |
| Bookings | info | Created, cancelled, approved, declined, reminder sent |
| CalDAV | info/error | Sync completed, write-back/delete failures, source added/removed |
| Admin | info/warn | Role changes, user toggle, config updates, impersonation |
| Email | debug/error | Delivery success/failure |
| HTTP | info | Every request (method, path, status, latency) |
| Database | info | Migrations applied on startup |

### Viewing logs

```bash
# systemd
journalctl -u calrs -f

# Docker
docker logs -f calrs
```

## Backup

A complete backup includes **`calrs.db`, the credential key and uploaded files**. The key is normally `$CALRS_DATA_DIR/secret.key`; if `CALRS_SECRET_KEY` supplies it, preserve that external secret instead. Without the key, stored calendar passwords, OAuth credentials and MFA secrets cannot be decrypted.

To make a consistent database snapshot while the server runs:

```bash
sqlite3 /var/lib/calrs/calrs.db ".backup /path/to/backup.db"
```

Also copy the key and uploaded assets, and retain service/environment configuration and the deployed version. For a filesystem backup of the whole data directory, stop calrs first and copy the directory, preserving ownership and any WAL files. Do not copy only a live SQLite main file while writes continue. Restore into a stopped instance with the matching key, then check access and calendar sync. See [Upgrading to v2](./upgrading.md) for rollback.
