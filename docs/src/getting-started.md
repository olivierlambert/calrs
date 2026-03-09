# Getting Started

## Installation

See [Deployment](./deployment.md) for Docker, systemd, and binary install options.

For development:

```bash
cargo build --release
```

## First-time setup

### Option 1: Web UI (recommended)

1. Start the server:
   ```bash
   calrs serve --port 3000
   ```
2. Open `http://localhost:3000` in your browser
3. Register an account — the first user automatically becomes admin
4. From the dashboard, add a CalDAV source and create your first event type

### Option 2: CLI

```bash
# Initialize your account
calrs init

# Connect your CalDAV calendar
calrs source add --url https://nextcloud.example.com/remote.php/dav \
                 --username alice --name "My Calendar"

# Pull events
calrs sync

# Create a bookable meeting type
calrs event-type create --title "30min intro call" --slug intro --duration 30

# Check available slots
calrs event-type slots intro

# Start the web server
calrs serve --port 3000
```

## Environment variables

| Variable | Description | Default |
|---|---|---|
| `CALRS_DATA_DIR` | Directory for the SQLite database | Platform-specific (XDG) |
| `CALRS_BASE_URL` | Public URL (needed for OIDC callbacks) | `http://localhost:3000` |

## Data directory

calrs stores everything in a single SQLite database (`calrs.db`) inside the data directory. By default this follows XDG conventions:

- **Linux:** `~/.local/share/calrs/`
- **macOS:** `~/Library/Application Support/calrs/`

Override with `CALRS_DATA_DIR` or `--data-dir`.

## Quick test

After setup, your booking page is available at:

- `/u/yourname` — your profile listing all event types
- `/u/yourname/intro` — the slot picker for the "intro" event type
