# New migration

Run `scripts/new-migration.sh short_description`. It creates
`migrations/YYYYMMDDHHMMSS_short_description.sql` with the current UTC time
(#333). `build.rs` picks the file up; nothing else to register.

`scripts/check-migrations.sh origin/main` checks that the migrations a branch
adds sort after the newest one on `main`, and that none of `main`'s is deleted
or renamed. CI runs it on every pull request and push.

# Functional checks

## Booking timezones

Run `cargo build`, then `python3 scripts/test-booking-timezones.py`.

The script uses only the Python standard library. It starts isolated real calrs
servers with `TZ=UTC` and `TZ=Europe/Berlin`, checks HTTP booking creation,
dashboard display, calendar downloads, timezone-setting changes, rescheduling,
cancellation, and CLI creation, and compares the resulting timestamps. It also
checks that a legacy record remains byte-for-byte unchanged. No real calendars
or SMTP services are contacted. Set `CALRS_BINARY` to test another binary.

Temporary databases and logs are retained under the printed `/tmp` path; the
script shuts down its servers on success or failure.

# Booking directory browser test

Run `cargo build`, then `node scripts/test-directory-browser.mjs` with Node.js
22+, Python 3 and Chrome installed. The test uses an isolated database and browser
profile. It checks search, personal favorites, dashboard shortcuts, copy actions,
fresh internal invites, clipboard denial, failed saves, live title/URL and
visibility changes, mobile layout and French translations. Screenshots and logs
are retained under the printed `/tmp` path. `CALRS_BINARY` and `CHROME_BINARY`
override executable paths.

The feature adds migration `078_event_type_favorites`, which only creates a
table and index. It runs automatically at startup, requires no backfill and
leaves existing tables unchanged. Older code can ignore the new table; favorites
remain stored when switching back and forth between versions. Deploy the binary
and templates from the same branch.

# MFA browser test

Run from the repository root with Node.js 22+ and Chrome installed:

```sh
cargo build
node scripts/test-mfa-browser.mjs
```

The test starts a local server and headless Chrome using a fresh temporary data
directory. It exercises optional enrollment and disable, password confirmation,
TOTP and recovery login, replay rejection, recovery-code replacement, mandatory
MFA, registration gating, and CLI recovery. It stops both processes afterward
and prints the directory containing screenshots and logs. Existing instance
data is not used. `CALRS_BINARY` and `CHROME_BINARY` override executable paths.

Rust coverage includes RFC 6238 vectors, expiry, replay, concurrent consumption,
encryption failures, account limits, CSRF, impersonation, and OIDC exemption:

```sh
cargo test mfa::
cargo test
cargo clippy --profile=test -- -D warnings
cargo fmt --check
```

The full Rust suite needs permission to bind local sockets and resolve DNS for
existing SMTP and calendar-source tests.

# Calendar week-start browser test

Run `cargo build`, then `node scripts/test-week-start-browser.mjs` with Node.js
22+ and Chrome installed. It uses an isolated server and browser profile to
check all seven first-day choices, inherited availability-editor order,
unchanged saved hours, copy-to-all in displayed order (including disabled days),
month grid alignment, and week navigation across month boundaries. It also
checks the column view and browser exceptions. No npm
packages or real calendars are required. The server and browser stop afterward;
the printed temporary directory contains the logs and a screenshot.

## Calendar OAuth

Run `cargo build`, then `python3 scripts/test-calendar-oauth.py`.

The script uses the Python standard library and an isolated real calrs server.
It imports synthetic Google and Microsoft client credentials through the CLI,
checks encrypted storage and secret-free configuration dumps, registers two
local users through HTTP, and exercises Microsoft CSRF protection, PKCE,
requested scopes, account-bound state, localized consent denial (the state
row is gone after the denied callback), and replay rejection, reading the
stored state through its own SQLite connection.
Trace-level logs are checked for callback-query and client-secret leakage.
That `consume_state`, which the callback calls, commits the consumed state
before it returns is checked by the Rust test
`consumed_state_is_committed_before_callback_returns`, which calls it
directly rather than going through the callback.

No external redirect is followed and no provider is contacted. This checks
local authorization boundaries, not Microsoft consent, token exchange, or
authenticated calendar interoperability. Use the provider guides for live
account verification. `CALRS_BINARY` selects another executable. The script
removes its temporary database and logs and stops its server on exit.
