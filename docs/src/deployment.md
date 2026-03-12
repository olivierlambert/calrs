# Deployment

## Docker / Podman (recommended)

Pre-built images are available on [GitHub Container Registry](https://github.com/olivierlambert/calrs/pkgs/container/calrs) for `amd64` and `arm64`:

```bash
docker run -d --name calrs \
  -p 3000:3000 \
  -v calrs-data:/var/lib/calrs \
  -e CALRS_BASE_URL=https://cal.example.com \
  ghcr.io/olivierlambert/calrs:latest
```

> **Podman** works as a drop-in replacement — just use `podman` instead of `docker` in all commands. The Containerfile (Dockerfile) is compatible with both runtimes.

To pin to a specific version: `ghcr.io/olivierlambert/calrs:0.14.0`

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
    image: ghcr.io/olivierlambert/calrs:latest
    ports:
      - "3000:3000"
    volumes:
      - calrs-data:/var/lib/calrs
    environment:
      - CALRS_BASE_URL=https://cal.example.com
    restart: unless-stopped

volumes:
  calrs-data:
```

Works with both `docker compose` and `podman-compose`.

## Binary + systemd

```bash
# Build from source
cargo build --release

# Install binary and templates
sudo cp target/release/calrs /usr/local/bin/
sudo cp -r templates /var/lib/calrs/templates

# Create a system user
sudo useradd -r -s /bin/false -m -d /var/lib/calrs calrs

# Install the service
sudo cp calrs.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now calrs
```

Edit `/etc/systemd/system/calrs.service` to set `CALRS_BASE_URL`.

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
cargo build --release
calrs serve --port 3000
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
| `CALRS_DATA_DIR` | SQLite database directory | `/var/lib/calrs` (Docker/systemd) or XDG (dev) |
| `CALRS_BASE_URL` | Public URL (required for OIDC callbacks and email action links) | `http://localhost:3000` |
| `RUST_LOG` | Log level filter | `calrs=info,tower_http=info` |

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

The entire state is in a single SQLite file (`calrs.db`). To back up:

```bash
sqlite3 /var/lib/calrs/calrs.db ".backup /path/to/backup.db"
```

Or simply copy the file when the server is stopped.
