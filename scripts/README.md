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
