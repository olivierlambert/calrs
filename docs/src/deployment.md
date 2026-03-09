# Deployment

## Docker / Podman (recommended)

```bash
docker build -t calrs .
docker run -d --name calrs \
  -p 3000:3000 \
  -v calrs-data:/var/lib/calrs \
  -e CALRS_BASE_URL=https://cal.example.com \
  calrs
```

> **Podman** works as a drop-in replacement — just use `podman` instead of `docker` in all commands. The Containerfile (Dockerfile) is compatible with both runtimes.

The image uses a multi-stage build:

- **Builder:** `rust:bookworm` — compiles the release binary
- **Runtime:** `debian:bookworm-slim` — minimal image with only `ca-certificates`
- Runs as unprivileged `calrs` user
- Data stored in `/var/lib/calrs`
- Templates bundled at `/opt/calrs/templates/`

## Docker Compose / Podman Compose

```yaml
services:
  calrs:
    build: .
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
calrs init
calrs serve --port 3000
```

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
| `CALRS_BASE_URL` | Public URL (required for OIDC callbacks) | `http://localhost:3000` |

## Backup

The entire state is in a single SQLite file (`calrs.db`). To back up:

```bash
sqlite3 /var/lib/calrs/calrs.db ".backup /path/to/backup.db"
```

Or simply copy the file when the server is stopped.
