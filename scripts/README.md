# Functional checks

Run `cargo build`, then `python3 scripts/test-booking-timezones.py`.

The script uses only the Python standard library. It starts isolated real calrs
servers with `TZ=UTC` and `TZ=Europe/Berlin`, checks HTTP booking creation,
dashboard display, calendar downloads, timezone-setting changes, rescheduling,
cancellation, and CLI creation, and compares the resulting timestamps. It also
checks that a legacy record remains byte-for-byte unchanged. No real calendars
or SMTP services are contacted. Set `CALRS_BINARY` to test another binary.

Temporary databases and logs are retained under the printed `/tmp` path; the
script shuts down its servers on success or failure.

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
