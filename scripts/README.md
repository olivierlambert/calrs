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

## Calendar OAuth

Run `cargo build`, then `python3 scripts/test-calendar-oauth.py`.

The script uses the Python standard library and an isolated real calrs server.
It imports synthetic Google and Microsoft client credentials through the CLI,
checks encrypted storage and secret-free configuration dumps, registers two
local users through HTTP, and exercises Microsoft CSRF protection, PKCE,
requested scopes, account-bound state, localized consent denial, and replay
rejection, reading the stored state through its own SQLite connection.
Trace-level logs are checked for callback-query and client-secret leakage.
That state consumption is committed before the callback returns is checked by
the Rust test `consumed_state_is_committed_before_callback_returns`.

No external redirect is followed and no provider is contacted. This checks
local authorization boundaries, not Microsoft consent, token exchange, or
authenticated calendar interoperability. Use the provider guides for live
account verification. `CALRS_BINARY` selects another executable. The script
removes its temporary database and logs and stops its server on exit.
