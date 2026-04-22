# calrs

**Fast, self-hostable scheduling. Like Cal.com, but written in Rust.**

calrs is an open-source scheduling platform. Connect your CalDAV calendar (Nextcloud, Fastmail, BlueMind, iCloud...), define bookable meeting types, and share a link. No Node.js, no PostgreSQL, no subscription.

## Key features

- **CalDAV sync** — pull events from any CalDAV server for free/busy computation, with multi-VEVENT support for recurring event modifications
- **CalDAV write-back** — confirmed bookings are automatically pushed to your calendar
- **Availability engine** — computes free slots from availability rules + calendar events
- **Recurring events** — RRULE expansion (DAILY/WEEKLY/MONTHLY with INTERVAL, UNTIL, COUNT, BYDAY, EXDATE) blocks availability correctly
- **Event types** — bookable meeting templates with duration, buffers, minimum notice
- **Booking flow** — public slot picker, booking form, email confirmations with `.ics` invites
- **Email approve/decline** — approve or decline pending bookings directly from the notification email
- **HTML emails** — clean, responsive HTML notifications with plain text fallback
- **Teams** — unified scheduling across team members with round-robin or collective modes
- **Timezone support** — guest timezone picker with browser auto-detection; CalDAV events are converted from their original timezone to your host timezone, so availability is always accurate regardless of where your calendar events were created
- **Authentication** — local accounts (Argon2) or OIDC/SSO (Keycloak, Authentik, etc.)
- **Web dashboard** — manage event types, calendar sources, pending approvals, bookings
- **Dark/light theme** — manual toggle (System/Light/Dark) on public pages and dashboard settings
- **Admin panel** — user management, auth settings, OIDC config, SMTP status, impersonation
- **Structured logging** — `tracing` + `tower-http` for production observability, configurable via `RUST_LOG`
- **Three-level visibility** — public (listed on profile), internal (any team member generates invite links for external contacts), private (invite-only by owner)
- **Availability overrides** — block specific dates or set custom hours per event type
- **Security hardening** — CSRF protection, booking rate limiting, input validation, double-booking prevention
- **Availability troubleshoot** — visual timeline showing why slots are blocked
- **SQLite storage** — single-file WAL-mode database, zero ops
- **Markdown descriptions** — bold, italic, links, and inline code in user bio, event type descriptions, and team descriptions. Formatting toolbar with live preview on all description fields
- **Onboarding** — getting-started checklist and guided action cards on the dashboard overview
- **Single binary** — no runtime dependencies

![calrs dashboard](images/dashboard.png)

## How it works

1. Connect your CalDAV calendar (or multiple calendars)
2. Sync events so calrs knows when you're busy
3. Create event types with your availability schedule
4. Share your booking link (`/u/yourname/meeting-slug`)
5. Guests pick a slot, fill in their details, and book
6. Both parties get an email with a calendar invite
7. The booking appears on your CalDAV calendar automatically

## License

AGPL-3.0 — free to use, modify, and self-host.
