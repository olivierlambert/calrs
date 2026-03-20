# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/), and this project adheres to [Semantic Versioning](https://semver.org/).

## Features at a glance

| Feature | Version | Description |
|---|---|---|
| CalDAV sync | 0.1.0 | Connect Nextcloud, BlueMind, Fastmail, iCloud, Google, etc. |
| Availability engine | 0.1.0 | Free/busy computation from availability rules + calendar events |
| Event types | 0.1.0 | Bookable meeting templates with duration, buffers, minimum notice |
| SQLite storage | 0.1.0 | Single-file WAL-mode database, zero ops |
| CLI | 0.1.0 | Full command set: init, source, sync, calendar, event-type, booking |
| Booking with conflict detection | 0.1.2 | Validates against both calendar events and existing bookings |
| Email notifications | 0.1.3 | SMTP emails with `.ics` calendar invites (REQUEST/CANCEL) |
| SMTP configuration | 0.1.3 | `calrs config smtp` — stored in SQLite, optional |
| Web booking page | 0.2.0 | Axum server with slot picker, booking form, confirmation page |
| Server-side slot computation | 0.2.0 | Same availability engine as CLI, exposed via HTTP |
| Local authentication | 0.3.0 | Email/password with Argon2, server-side sessions, HttpOnly cookies |
| User roles | 0.3.0 | Admin/user with extractors, first user becomes admin |
| User management CLI | 0.3.0 | `calrs user create/list/promote/demote/set-password` |
| Registration controls | 0.3.0 | Enable/disable registration, restrict by email domain |
| User-scoped URLs | 0.3.0 | Public pages at `/u/{username}` and `/u/{username}/{slug}` |
| Booking cancellation | 0.3.0 | Cancel from dashboard with optional reason + email notification |
| Pending bookings | 0.3.0 | `requires_confirmation` — host approves/declines from dashboard |
| Web dashboard | 0.3.0 | Event types, pending approvals, upcoming bookings |
| OIDC authentication | 0.3.1 | SSO via Keycloak (authorization code + PKCE, auto-discovery) |
| Admin dashboard | 0.3.1 | User management, auth settings, OIDC config, SMTP status |
| Event type management UI | 0.3.1 | Create/edit from dashboard with availability, location, confirmation |
| Location support | 0.3.1 | Video link, phone, in-person, or custom — in pages, emails, `.ics` |
| OIDC group sync | 0.3.2 | Groups synced from Keycloak `groups` JWT claim on SSO login |
| Group event types | 0.4.0 | Combined availability (any member free) + round-robin assignment |
| Public group pages | 0.4.0 | `/g/{group-slug}` and `/g/{group-slug}/{slug}` |
| Timezone support | 0.4.0 | Guest timezone picker, browser auto-detection, tz-aware booking |
| Calendar source management UI | 0.5.0 | Add/test/sync/remove CalDAV sources from the web dashboard |
| Provider presets | 0.5.0 | BlueMind, Nextcloud, Fastmail, iCloud, Google, etc. with auto-fill |
| Docker image | 0.5.1 | Multi-stage Dockerfile, docker-compose example |
| systemd service | 0.5.1 | Production-ready unit file with security hardening |
| CalDAV write-back | 0.6.0 | Push confirmed bookings to host's calendar, delete on cancel |
| Login rate limiting | 0.6.1 | Per-IP rate limiting on login attempts |
| Secure cookies | 0.6.1 | HttpOnly + Secure flag on all session cookies |
| ICS sanitization | 0.6.1 | Prevents injection in calendar invites |
| RRULE expansion | 0.7.0 | Recurring events block availability (DAILY/WEEKLY/MONTHLY, EXDATE) |
| Availability troubleshoot | 0.7.0 | Visual timeline showing why slots are blocked |
| Duplicate email fix | 0.7.0 | Guest emails use METHOD:PUBLISH to avoid mail server re-invites |
| RECURRENCE-ID handling | 0.7.1 | Modified recurring event instances no longer cause phantom occurrences |
| Admin impersonation | 0.8.0 | Admins can impersonate any user for troubleshooting |
| HTML emails | 0.8.3 | Clean, responsive HTML email notifications with plain text fallback |
| Multi-VEVENT sync | 0.8.4 | Recurring events with modified instances properly synced from CalDAV |
| Email approve/decline | 0.8.5 | Approve or decline pending bookings directly from the notification email |
| Timezone-aware CalDAV events | 0.9.0 | Event times converted from their calendar timezone to host timezone for accurate availability |
| Credential encryption | 0.10.0 | AES-256-GCM encryption for stored CalDAV/SMTP passwords |
| Per-event-type calendar selection | 0.11.0 | Choose which calendars block availability per event type |
| Guest self-cancellation | 0.12.0 | Guests can cancel their own bookings via a link in the confirmation email |
| Booking reminders | 0.13.0 | Automated email reminders before meetings (configurable per event type) |
| User settings | 0.16.0 | Display name editing and booking email override |
| Event type deletion | 0.16.0 | Delete event types from dashboard (blocked when active bookings exist) |
| Smart onboarding | 0.16.0 | Calendars sorted by event count on write-back setup |
| Ad-hoc team links | 0.17.0 | Shareable booking links across hand-picked users, all-must-be-free scheduling |
| Sidebar navigation | 0.17.0 | Persistent left sidebar with organized nav sections, mobile hamburger menu |
| User profile | 0.17.0 | Avatar upload, title, bio — shown in sidebar and public booking pages |
| Dashboard pages | 0.17.0 | Split monolithic dashboard into focused Event Types, Bookings, Sources, Team Links pages |
| Human-friendly dates | 0.17.2 | Booking dates shown as "Tomorrow at 2:30 PM" instead of raw timestamps |
| Mobile improvements | 0.17.2 | Responsive booking rows, event type listings, form grids on small screens |
| Host identity on bookings | 0.17.1 | Avatar, name, and title shown on slot picker for individual bookings |
| Team link search UX | 0.17.1 | Search + pill selection for team members with avatar previews |
| Matrix-style initials | 0.17.1 | Two-letter avatar fallback (first+last name initials) across all pages |
| Multiple availability windows | 0.18.0 | Define morning + afternoon slots with lunch breaks (multiple time windows per event type) |
| Calendar reminders (VALARM) | 0.18.1 | ICS events include native calendar reminders (popup/notification) based on event type settings |
| ICS timezone fix | 0.18.2 | ICS events use UTC times with Z suffix instead of floating times |
| Version in sidebar | 0.18.2 | calrs version displayed in the dashboard sidebar |
| CSRF protection | 0.19.0 | Double-submit cookie pattern on all 31 POST handlers |
| Booking rate limiting | 0.19.0 | Per-IP rate limiting on all booking endpoints (10 req / 5 min) |
| Input validation | 0.19.0 | Server-side validation on all user-submitted data |
| Double-booking prevention | 0.19.0 | SQLite unique index + transactions prevent race conditions |
| Crash-proof handlers | 0.19.0 | All web handler `.unwrap()` replaced with proper error handling |
| Graceful shutdown | 0.19.0 | SIGINT/SIGTERM handling with in-flight request draining |
| Structured logging | 0.19.0 | 50 tracing points across auth, bookings, CalDAV, admin, email |
| Regression tests | 0.19.0 | 28 new tests (191 → 219) covering ICS, validation, CSRF |
| ICS attendee names | 0.19.0 | Calendar events show "{title} — {guest} & {host}" with guest notes in description |
| Host confirmation email | 0.19.0 | Host receives booking confirmed email (without ICS) after approving pending bookings |
| UX polish | 0.20.0 | Clickable dashboard cards, hover animations, status badges, gradient profile header, admin search/filter |
| ICS time fix | 0.19.0 | Correct UTC times in ICS when confirming/cancelling bookings from the database |
| Private event types | 0.21.0 | Hide event types from public profile, accessible only via invite links |
| Booking invites | 0.21.0 | Send tokenized invite links with pre-filled guest info, expiration, and usage limits |
| Cal.com-style slot picker | 0.21.0 | Month calendar with 3-panel layout, meeting info sidebar |
| Reusable team links | 0.21.0 | Team links are reusable by default, with opt-in one-time use |
| Team link editing | 0.21.0 | Edit existing team links (title, duration, members, settings) |
| Dark/light theme toggle | 0.21.0 | Manual theme switching on public pages and in dashboard settings |
| Additional attendees | 0.21.0 | Guests can invite additional people to bookings (configurable per event type) |
| Stale event cleanup | 0.21.0 | Cancelled and deleted CalDAV events removed from local cache |
| Theme engine | 0.21.1 | 7 preset themes (Default, Nord, Dracula, Gruvbox, Solarized, Tokyo Night, Vates) + custom colors |
| Improved slot picker UX | 0.21.1 | Dynamic TZ offsets, filled calendar grid, sidebar controls, clickable prev/next month days |
| Reschedule | 0.22.0 | Guests and hosts can reschedule bookings — new slot picker, CalDAV update in place, token regeneration |
| Host reschedule UX | 0.23.0 | Host-initiated reschedule confirmed without re-approval, reschedule from pending bookings |
| Availability overrides | 0.24.0 | Block specific dates or set custom hours per event type |
| Three-level visibility | 0.24.0 | Public / internal (any team member generates invite links) / private (owner-only invites) |
| Organization dashboard | 0.24.0 | Internal event types listed for all team members with one-click invite link generation |
| CalDAV sync-token (RFC 6578) | 0.25.0 | Delta sync with ctag comparison — O(changes) instead of O(total events) |
| Admin group management | 0.25.0 | Admins can create/edit/delete group event types without being a group member |
| Background calendar sync | 0.25.0 | Automatic source cycling in the reminder loop, one source per tick |
| External cancellation detection | 0.25.0 | Bookings auto-cancelled when their CalDAV event is deleted externally |
| Self-hosted fonts | 0.25.3 | Inter font bundled in binary — no external requests to Google, fully GDPR-compliant |
| Markdown bio | 0.26.0 | Links, bold, italic in user bio via Markdown syntax |
| Company link | 0.26.1 | Logo on public pages links to configurable company URL |
| Unified Teams | 1.0.0 | Groups + team links merged into a single Teams concept |
| Personal Internal visibility | 1.0.0 | Any colleague can generate invite links for personal event types |
| Markdown everywhere | 1.0.0 | Toolbar + rendering on all description fields (bio, event type, team) |
| UX overhaul | 1.0.0 | Onboarding, unified event types page, badge system, AJAX navigation |
| Calendar view toggle | 1.1.0 | Month, week, and column views on the slot picker (Cal.com-style) |
| Default calendar view | 1.1.0 | Per-event-type default view setting (month/week/column) |
| Booking frequency limits | 1.1.0 | Cap bookings per day/week/month/year per event type |
| One slot per day | 1.1.0 | Show only the earliest available time each day |
| Event type form redesign | 1.1.0 | Reorganized into focused cards: Booking Options, Access, Notifications |

## [1.1.0] - 2026-03-20

New guest-facing calendar views and host-side booking controls.

### Added

- **Calendar view toggle** — guests can switch between month grid, week columns, and column (list) views on the slot picker. Preference persisted in `localStorage`. SVG toggle icons in the calendar header, Cal.com-inspired layouts
- **Default calendar view** — hosts can set which view guests see first (month/week/column) per event type. Guest's explicit choice still takes priority
- **Booking frequency limits** — cap how many times an event type can be booked per day, week, month, or year. Multiple limits can be combined (e.g., max 2/day AND 8/week). Toggle-based UI with dynamic rows
- **One slot per day** — toggle to show only the earliest available time slot each day. Useful for daily standups or check-ins
- **Event type form reorganized** — the old "Notifications & Access" card split into focused sections:
  - **Booking limits** — one slot per day + frequency limits (toggle switches)
  - **Booking options** — requires confirmation, additional guests, default calendar view
  - **Access** — visibility (public/internal/private)
  - **Notifications** — reminder settings

### Fixed

- Calendar view toggle buttons disappearing when switching views (moved to shared header)
- Today indicator in week view breaking column layout (now uses accent dot instead of circle)
- Week view showing year only instead of month name in header
- Column (list) view prev/next arrows not working after month navigation
- Frequency limit toggle enabled by default on new event types
- Private team avatars returning 404
- Booking invite token rejected on private/internal team events
- `scheduling_mode` not saved when updating event types
- Any team member can now edit/toggle/delete team event types

## [1.0.0] - 2026-03-19

The first stable release. Major UX overhaul and unified teams architecture.

### Breaking changes

- **Unified Teams** — Groups (OIDC scheduling units) and team links (ad-hoc booking links) have been replaced by a single **Teams** concept. Migration 034 automatically converts existing data. **Back up your database before upgrading** — migration 035 drops legacy tables.
- **Personal profile pages** no longer show team event types — they belong on the team profile page (`/team/{slug}`).
- **Sidebar renamed**: "Organization" → "Shared Links", "Internal Bookings" → "Invite Links".

### Added

- **Unified Teams** — create teams from OIDC groups, individual users, or both. Public/private visibility, team avatars, stacked member avatars on booking pages
- **Personal Internal visibility** — "Internal" is no longer restricted to team event types. Any colleague can generate invite links from the Invite Links page
- **Quick link button** on invite management page — one-click link generation + clipboard copy, no email required
- **Onboarding checklist** on dashboard overview (connect calendar → create event type → share link)
- **Dashboard reordered** — pending bookings first (most urgent), stats tiles second, action cards last
- **Unified event types page** — personal and team event types merged into one list with team name badges and slug preview
- **Markdown toolbar** on all description fields (bio, event type, team) with Bold/Italic/Strikethrough/Code/Link + Preview toggle
- **Inline markdown rendering** on all public-facing descriptions
- **Badge system** — `.badge-success/warning/info/muted/error` classes replacing inline styles across all templates
- **AJAX navigation** — troubleshoot page swaps content without reload; slots page rebuilds calendar in place on month change
- **Focus indicators** (`:focus-visible`) for keyboard accessibility on all interactive elements
- **Text contrast** bumped to WCAG AA compliance
- **Mobile calendar** responsive at <400px (compact cells, single-letter day labels)
- **Action dropdown** ("⋯") on event types listing for secondary actions
- **Admin panel** — user actions visible on hover, group members collapse after 5
- **Empty states** with actionable CTAs on all pages
- **Public booking URL** with copy button on Settings page
- **Scheduling mode help text** on event type form
- **Per-event-type member priority** card shown during creation (not just editing)
- **Excluded members** (weight=0) hidden from booking page avatars
- **Global admins** can remove themselves from teams (IT admin use case)
- **Description truncation** (2-line clamp) on profile/team listing pages
- **Theme-aware gradients** — profile/team headers use CSS variables instead of hardcoded colors
- **Source form** preserves user-entered URL on error re-render
- **BlueMind help** — note that username is your email address
- **Confirmation page CTAs** — "Book another time" / "You can close this page"
- **Shared `calrsFormat12h()`** — deduplicated 12h time format across 4 templates

### Fixed

- Saving event type no longer silently converts internal → private
- "View public page" hidden for private event types
- Internal preset from dashboard auto-selects team and shows visibility option
- Troubleshoot AJAX navigation no longer targets wrong form element
- Slots month navigation no longer gets stuck on "Checking availability" loader
- Migration handles slug collisions (team links with duplicate titles)
- Migration handles NULL `created_by_user_id` (deleted creators)
- Member priorities validated against actual team members on creation

## [0.26.1] - 2026-03-18

### Added

- **Company link** — admins can set a company URL in the admin panel (next to logo upload). When set, the company logo on all public booking pages becomes a clickable link opening in a new tab. Cached in memory for zero-query reads. ([#24](https://github.com/olivierlambert/calrs/pull/24))

## [0.26.0] - 2026-03-18

### Added

- **Markdown bio** — user bio on public profile pages now supports Markdown inline elements: `[text](url)` links, **bold**, *italic*, ~~strikethrough~~, and `inline code`. Block elements (headings, images, lists, raw HTML) are stripped for safety. Links open in new tabs. Settings form shows syntax hint. ([#21](https://github.com/olivierlambert/calrs/pull/21))

## [0.25.3] - 2026-03-18

### Added

- **Self-hosted Inter font** — Inter WOFF2 font files are now bundled in the binary and served from `/fonts/`, eliminating all external requests to Google Fonts. Fully self-hosted, no third-party CDN dependencies, GDPR-compliant ([#19](https://github.com/olivierlambert/calrs/issues/19)).

## [0.25.2] - 2026-03-17

### Added

- **Favicon** — crab emoji (🦀) favicon on all pages via inline SVG (#16)

### Fixed

- **Date overrides responsive layout** — header, radio buttons, and override list items now wrap properly on mobile instead of overflowing (#17)

## [0.25.1] - 2026-03-17

### Added

- **Cancellation email on external deletion** — when a CalDAV event is deleted externally (e.g. in BlueMind) and the booking is cancelled, both guest and host now receive a cancellation email with `.ics` CANCEL attachment

### Fixed

- **BlueMind empty sync-collection** — if ctag changed but sync-collection returns an empty delta (BlueMind doesn't report deletions via sync-token), calrs now falls back to full sync to catch the changes
- **Orphaned booking sweep** — after every sync, calrs checks for active bookings whose CalDAV event no longer exists and cancels them. This catches bookings orphaned before the cancellation detection was deployed

## [0.25.0] - 2026-03-17

### Added

- **CalDAV sync-token (RFC 6578)** — efficient delta sync replaces full-fetch. ctag comparison skips unchanged calendars entirely. sync-collection REPORT fetches only additions, modifications, and deletions since the last token. Automatic fallback to full fetch for servers that don't support RFC 6578. Makes sync O(changes) instead of O(total events), critical for scaling to hundreds of users
- **Admin group management** — admins can now create, edit, toggle, and delete event types for any group, even if they are not a member. Lets IT teams configure group meetings on behalf of other teams without joining every group
- **Background calendar sync** — the reminder loop (every 60s) now also syncs the stalest CalDAV source each tick. With ctag + sync-token, this is near-instant for unchanged calendars but catches deletions even when nobody visits the slot page
- **External cancellation detection** — when sync detects a CalDAV event was deleted on the server side (e.g. deleted in BlueMind), the corresponding calrs booking is automatically marked as cancelled
- **`calrs sync --full` flag** — forces a full re-sync by clearing stored sync-tokens and ctags

### Changed

- **Unified web/CLI sync** — the web dashboard sync handler now delegates to the shared `sync_source()` function instead of duplicating ~100 lines of sync logic
- **On-demand sync uses sync-token** — `sync_if_stale()` now uses the same ctag + sync-token path instead of time-range filtering, making it both faster and able to detect deletions

### Database

- Migration 027: `sync_token TEXT` column added to `calendars` table

## [0.24.0] - 2026-03-14

### Added

- **Availability overrides** — block specific dates (day off) or set custom hours per event type from `/dashboard/event-types/{slug}/overrides`. Overrides replace weekly rules for that day. Multiple custom hour windows supported. Visible in troubleshoot view
- **Three-level visibility** — event types can be public (listed on profile), internal (group only — any team member generates invite links), or private (owner sends invite links). Replaces the binary `is_private` flag
- **Organization dashboard** — `/dashboard/organization` lists all internal event types across the org. "Get link" button generates a single-use invite URL (7-day expiry) and copies to clipboard. "Invites" link for full invite management
- **Quick invite link generation** — `POST /dashboard/invites/{id}/quick-link` creates a single-use invite and returns JSON with the URL for clipboard copy
- **Animated theme toggle** — pill-shaped dark/light slider with SVG sun/moon icons, fixed top-right on all public pages
- **Integration test harness** — `setup_test_app()` with in-memory SQLite, session auth, and `tower::ServiceExt::oneshot` for HTTP handler testing

### Fixed

- **Register link** — hidden on login page when registration is disabled
- **Reschedule confirmation** — host-initiated reschedules show "Rescheduled!" instead of misleading pending message
- **Reschedule from pending** — hosts can reschedule bookings before approving them

### Tests

- **247 → 496 tests** covering web handlers (GET + POST), CLI commands, auth lifecycle, email HTML/ICS builders, config commands, booking validation, CSRF, rate limiting, admin actions, token-based approve/decline/cancel, reschedule flow, overrides CRUD, double-booking prevention, and more
- Fixed test DB pool deadlock (`max_connections: 1` → `2`)

### Documentation

- Five distinct meeting types documented with use cases (README + mdBook)
- Multi-timezone group setup guide (wide availability window approach)
- Visibility levels, availability overrides, Organization dashboard

## [0.23.0] - 2026-03-14

### Added

- **Reschedule pending bookings** — hosts can now reschedule a booking that is still pending approval, suggesting a different time instead of declining outright

### Fixed

- **Reschedule confirmation page** — host-initiated reschedules now show "Rescheduled!" (confirmed) instead of the misleading "Reschedule requested" (pending) message
- **Reschedule UX** — awaiting reschedule badge, correct approval logic for host vs guest reschedule flows
- **Slot picker layout** — reschedule banner no longer breaks the slot picker
- **Meeting location** — hidden until booking is confirmed
- **Register link** — hidden on login page when registration is disabled

### Added (tests)

- Functional test suite with seeded data
- Template rendering regression tests for slot links

## [0.22.1] - 2026-03-14

### Fixed

- **Slot picker links broken** — clicking a time slot navigated to `/{username}/{` instead of the booking form. Caused by `{{ reschedule_base | default(value='') }}` in the template: minijinja interpreted the `value=''` named argument as creating an object `{"value": ""}` instead of an empty string default. Fixed by using `default('')`.

## [0.22.0] - 2026-03-14

### Added

- **Reschedule flow** — guests and hosts can reschedule bookings without cancelling and rebooking
  - Guest reschedule via tokenized link in confirmation/pending emails — picks a new slot, booking goes to pending for host approval
  - Host reschedule from the dashboard — picks a new slot, booking stays confirmed, no approval needed
  - Slot picker shows an amber banner ("Rescheduling: {title}") with current booking info and the booking's own slot freed for re-selection
  - Reschedule confirmation page with strikethrough old time, green new time, 12h format support
  - All tokens regenerated after each reschedule (reschedule, cancel, confirm) — invalidates old email links
  - CalDAV events updated in place (same UID) for host reschedule; deleted and re-pushed on approval for guest reschedule
  - `reminder_sent_at` cleared so reminders fire for the updated time
  - New email templates: guest reschedule notification (orange accent, updated ICS), host reschedule approval request (approve/decline buttons)
  - Existing confirmation and pending emails now include a "Reschedule" button alongside "Cancel"
  - `fetch_busy_times_for_user_ex()` supports `exclude_booking_id` to prevent self-conflict during reschedule
  - 4 new routes: `GET/POST /booking/reschedule/{token}`, `GET/POST /dashboard/bookings/{id}/reschedule`
  - New template: `booking_reschedule_confirm.html`
  - 18 new tests (225 → 243): token lookup, status filtering, token regeneration, self-conflict exclusion, host stays confirmed, reminder reset

### Improved

- **Dashboard bookings UX** — Reschedule button per booking; both action buttons hide when cancel form expands; cancel confirm button says "Confirm cancel"
- **Reschedule banner dark mode** — amber banner uses theme-aware colors instead of hardcoded light-only
- **Confirmed page** — guest reschedule shows "Reschedule requested" with dedicated icon instead of generic pending message

### Updated

- README: reschedule feature, test count (243+), roadmap checked off, new screenshots
- Documentation: reschedule section in booking-flow.md with guest/host flows, token regeneration, edge cases
- All screenshots refreshed with seeded data

## [0.21.1] - 2026-03-13

### Added

- **Theme engine** — full color theming from the admin dashboard
  - 7 preset themes: Default (blue), Nord (arctic frost), Dracula (dark purple), Gruvbox (retro warm), Solarized (classic), Tokyo Night (neon cityscape), Vates (Rouge & Bleu Spatial from official brand guidelines)
  - Custom theme: pick your own accent, accent hover, background, surface, and text colors via color pickers
  - Themes override all CSS custom properties (background, surface, text, accent, borders, success, error) for both light and dark modes
  - Served via `/accent.css` endpoint with 60s cache, cached in memory with `RwLock`
  - New migration: `theme`, `custom_accent`, `custom_accent_hover`, `custom_bg`, `custom_surface`, `custom_text` columns on `auth_config`
- **Dynamic timezone labels** — timezone picker shows UTC offsets computed at request time (DST-aware), e.g. "Paris, Brussels (UTC+1)"
- **Filled calendar grid** — previous and next month days fill empty calendar cells, clickable to navigate
- **Slot picker sidebar controls** — timezone selector and 12/24h toggle moved to left sidebar with "Your timezone" label
- **Floating theme toggle** — dark/light toggle as a floating button on the booking card

### Changed

- Replaced accent-only color swatches in admin with full theme card picker UI
- Removed redundant green availability dots from calendar days and slot pills

## [0.21.0] - 2026-03-13

### Added

- **Private event types** — mark any event type as "private" to hide it from your public profile and group pages. Private event types are only accessible via invite links.
- **Booking invites** — send personalized invite links for private event types
  - Invite management page at `/dashboard/invites/{event_type_id}` with sent invite list and status badges (active/expired/used)
  - Send invites with guest name, email, optional personal message, expiration (7/14/30 days or never), and single-use or multi-use toggle
  - Invite email sent via SMTP with indigo accent color, event details, and "Choose a time" CTA button
  - Tokenized URLs preserve the invite token through the full booking flow (slot picker → booking form → confirmation)
  - Guest name and email auto-filled from the invite data on the booking form
  - Token validated at every step: expired, used-up, or invalid tokens are rejected with a clear error
  - `used_count` incremented on successful booking
  - Works with both personal and group event types (round-robin assignment preserved)
  - Any user with dashboard access can create invites for private event types they can see (enables sales reps to invite guests to demo team event types)
  - New migration: `is_private` column on `event_types`, `booking_invites` table with token, expiration, usage tracking
- **Cal.com-style slot picker** — redesigned booking page with a 3-panel layout
  - Left sidebar with meeting info (host avatar, name, title, event details, duration, location)
  - Month calendar navigation (replaces week-by-week arrows)
  - Slot pills on the right, compact height for less scrolling
  - Responsive: stacks vertically on mobile
- **Reusable team links** — team links are now reusable by default (can be booked multiple times). Opt-in "one-time use" checkbox auto-deletes the link after a single booking. Existing one-time links are preserved via migration default.
- **Team link editing** — edit existing team links from the dashboard (title, duration, buffers, minimum notice, availability window, team members, one-time use toggle)
- **Dark/light theme toggle** — manual System/Light/Dark theme switcher
  - Public pages: sun/moon toggle in the footer, persisted in `localStorage`
  - Dashboard: appearance picker in Profile & Settings
  - Flash-free: inline `<head>` script applies theme before CSS loads
  - Defaults to system preference (`prefers-color-scheme`)
- **Additional attendees** — guests can invite additional people to bookings
  - Configurable per event type: 0, 1, 3, 5, or 10 max additional guests
  - Dynamic email input rows with add/remove on the booking form
  - Additional attendees stored in `booking_attendees` table
  - ICS calendar invites include ATTENDEE lines for all guests
  - Confirmation emails sent to each additional attendee with ICS attachment
  - Shown on the confirmation page
  - New migration: `max_additional_guests` on event types, `booking_attendees` table

### Fixed

- **Stale cancelled events** — cancelling a booking in calrs now also removes the cached event from the local database, so it no longer blocks availability in troubleshoot or slot computation
- **Stale deleted events on sync** — full sync (`calrs sync --full`) now compares local events against the server and deletes orphans that were removed remotely
- **Hidden meeting details before booking** — video call links and phone numbers are no longer visible on the public slot picker page (only shown after booking)
- **24h time selects** — availability time inputs in event type and team link forms now use 24h select dropdowns instead of free-text input
- **XSS in team link form** — replaced `innerHTML` with DOM methods for user-supplied data in the member search UI

## [0.20.4] - 2026-03-13

### Fixed

- **Shared event visibility** — recurring events synced by multiple users (attendees of the same meeting) were invisible to some users' availability. The unique constraint on events was global instead of per-calendar, causing `ON CONFLICT` upserts to overwrite the `calendar_id` to whichever user synced last. Now each user's calendar gets its own copy of the event.

### Added

- **12/24h time format toggle** — slot pages show a 24h/12h toggle (default: 24h), persisted in `localStorage`. Applies to slot picker, booking form, and confirmation page.
- **Minimum notice unit selector** — event type and team link forms now show a number + unit dropdown (minutes/hours/days) instead of raw minutes. Auto-detects the best unit when editing.
- **Group event type management** — Edit, Disable/Enable, and Delete buttons for group event types ([#11](https://github.com/olivierlambert/calrs/issues/11)).

## [0.20.3] - 2026-03-13

### Added

- **Group event type management** — Edit, Disable/Enable, and Delete buttons for group event types on the dashboard ([#11](https://github.com/olivierlambert/calrs/issues/11)). Previously group event types could only be viewed, not managed after creation.

## [0.20.2] - 2026-03-13

### Fixed

- **Slot ordering** — available time slots are now sorted by time within each day ([#10](https://github.com/olivierlambert/calrs/issues/10)). Previously, slots could appear out of order (e.g. afternoon before morning) when multiple availability windows were defined. Fixed in both web UI and CLI.

## [0.20.1] - 2026-03-13

### Changed

- **Sidebar redesign** — calrs logo + two-tone brand name ("cal" blue, "rs" orange) at top linking to dashboard; user profile moved to bottom in a compact row with inline sign-out icon; clicking name/avatar goes to settings
- **Inter font** — loaded from Google Fonts for consistent typography across platforms
- **Admin pagination** — users and groups lists paginated (5 per page) with prev/next navigation
- **Admin search fields** — pill-shaped rounded inputs with accent focus ring
- **Stat card watermark icons** — faint centered emoji backgrounds (4% opacity) for visual personality
- **Welcome card accent** — 2px blue top border on the dashboard welcome card
- **Button gradient** — primary buttons use a subtle diagonal gradient instead of flat color
- **Pressed states** — buttons scale down (0.97×) on click for tactile feedback
- **Brand logo route** — `/brand-logo` serves the calrs logo (compiled into the binary)

### Fixed

- **Page flash removed** — removed the fade-in animation that caused a white flash on navigation
- **Footer overlap** — "Powered by calrs" no longer renders under the sidebar on dashboard pages; hidden on authenticated pages, shown only on public pages
- **Footer link** — "Powered by calrs" now links to cal.rs website instead of GitHub repo

## [0.20.0] - 2026-03-13

### Added

- **Clickable dashboard cards** — stat tiles (Event Types, Upcoming Bookings, Pending Approval, Calendar Sources) are now links to their respective dashboard pages
- **Public page link opens in new tab** — the `/u/{username}` link on the dashboard overview now opens in a new tab
- **Admin search/filter** — users list has a live filter by name or email; groups list has a live filter by name
- **Status badges** — "disabled" and "requires confirmation" on event types are now colored pill badges (red/amber) instead of plain text; pending bookings show an amber "pending" badge
- **Card hover lift** — interactive cards (stat tiles, profile event types, group event types) lift with a shadow on hover
- **Page fade-in animation** — subtle 0.3s fade-in + slide-up on every page load
- **Slot button hover scale** — time slot buttons scale up slightly (1.03×) on hover for a tactile feel
- **Colored left border** — event type cards on public profile and group pages have a 3px accent-colored left border
- **Profile gradient banner** — public profile page has a blue-to-purple gradient header behind the avatar
- **Animated checkmark** — confirmation page checkmark bounces in with a scale animation
- **Better empty states** — empty listings (bookings, event types, slots) show a larger icon + descriptive text instead of a plain line
- **Rust crab branding** — "Powered by calrs" footer now includes the 🦀 emoji on all pages

## [0.19.1] - 2026-03-13

### Changed

- **Version link in sidebar** — calrs version in the dashboard sidebar now links to the GitHub release page for that version

## [0.19.0] - 2026-03-13

### Added

- **CSRF protection** — double-submit cookie pattern on all 31 POST handlers via middleware
- **Booking rate limiting** — per-IP rate limiting (10 req / 5 min) on all booking endpoints using `X-Forwarded-For`
- **Input validation** — server-side validation on all booking forms (name 1–255, email format, notes max 5000, date max 365 days)
- **Double-booking prevention** — partial unique index on `(event_type_id, start_at)` + `BEGIN IMMEDIATE` transactions
- **Crash-proof handlers** — all `.unwrap()` in web handlers replaced with proper error responses
- **Graceful shutdown** — SIGINT/SIGTERM handling with `with_graceful_shutdown()` to drain in-flight requests
- **Structured logging** — 50 `tracing` log points across auth, bookings, CalDAV, admin, email, DB migrations. Configurable via `RUST_LOG` (default: `calrs=info,tower_http=info`)
- **HTTP request tracing** — `tower-http` TraceLayer logs every request with method, path, status, and latency
- **ICS attendee names** — calendar event SUMMARY now shows "{title} — {guest_first} & {host_first}" (e.g. "30min call — John & Olivier") instead of just the event type title
- **ICS guest notes** — guest notes included as DESCRIPTION field in ICS calendar events
- **Host confirmation email** — host receives a "Booking confirmed" email (without ICS attachment) after approving a pending booking. Previously only the guest was notified.
- **32 new tests** (191 → 223) covering ICS generation, input validation, CSRF functions, time extraction

### Fixed

- **ICS times at midnight on confirm/cancel** — `format_time_from_dt()` returned 12-hour display format ("2:00 PM") but `convert_to_utc()` expected 24-hour "HH:MM", causing all ICS events generated from database bookings (confirm, approve, cancel, decline, reminders) to have midnight times with zero duration. Added `extract_time_24h()` helper.
- **Missing host email on booking approval** — both `confirm_booking` (dashboard) and `approve_booking_by_token` (email link) only sent the guest a confirmation email, never notifying the host.
- **Silent email failures** — `send_host_notification` errors were discarded via `let _ =`. Now logged at error level with the target email address.

## [0.18.2] - 2026-03-12

### Fixed

- **ICS location field corruption** — LOCATION line in `.ics` calendar invites had trailing whitespace after CRLF, causing the ORGANIZER field to be interpreted as a continuation of LOCATION per RFC 5545 line folding rules. BlueMind and other strict CalDAV servers displayed the organizer info inside the location field.
- **ICS floating times** — DTSTART/DTEND in `.ics` invites used floating times (no timezone) instead of UTC. Events appeared at the wrong time for guests in different timezones. Now converts to UTC with `Z` suffix via `convert_to_utc()`.
- **Hardcoded UTC guest timezone** — `confirm_booking` and `approve_booking_by_token` handlers passed `"UTC"` as guest timezone instead of the actual stored timezone, causing ICS times in approval emails to be wrong.
- **Broken "Add source" link on dashboard overview** — pointed to `/dashboard/sources/add` instead of `/dashboard/sources/new`

### Added

- **Version display in sidebar** — calrs version shown at the bottom of the dashboard sidebar

## [0.18.1] - 2026-03-11

### Added

- **Calendar reminders via VALARM** — booking ICS events now include a native calendar reminder (DISPLAY alarm) when the event type has `reminder_minutes` configured. The calendar app shows a popup notification before the meeting, working offline without SMTP. Applies to both email .ics attachments and CalDAV write-back. Closes #4.

## [0.18.0] - 2026-03-11

### Added

- **Multiple availability windows per event type** — define separate time blocks (e.g. 09:00–12:00 + 13:00–17:00) to create lunch breaks or custom schedules. Dynamic "Add time window" UI with add/remove buttons. Backward-compatible with existing single-window event types. Closes #5.

### Fixed

- **Post-action redirects go to correct dashboard page** — creating/deleting team links, event types, bookings, and sources now redirect to their respective page instead of the overview

## [0.17.6] - 2026-03-11

### Fixed

- **Post-action redirects go to correct dashboard page** — creating/deleting team links now redirects to `/dashboard/team-links` instead of the overview; same fix applied to event types (`/dashboard/event-types`), bookings (`/dashboard/bookings`), and sources (`/dashboard/sources`)

## [0.17.5] - 2026-03-11

### Improved

- **Test coverage** — added 35 tests for date formatting helpers, email HTML rendering, ICS generation (including injection prevention), and timezone parsing. Total: 147 → 182 tests.

## [0.17.4] - 2026-03-11

### Fixed

- **Team link creation "duplicate field" error** — switched to `axum_extra::extract::Form` (serde_html_form) for team link handler, since HTML checkboxes with the same name produce repeated keys that `serde_urlencoded` rejects

## [0.17.3] - 2026-03-11

### Fixed

- **Raw dates on token-based booking pages** — approve, decline, cancel confirmation and form pages now show human-friendly dates (e.g. "Saturday, March 15, 2026") instead of raw ISO8601 strings
- **Raw dates in reminder and cancellation emails** — time formatting now correctly parses both `T` and space datetime separators from the database

## [0.17.2] - 2026-03-11

### Fixed

- **Team link creation fails with single day selected** — form deserialization now handles HTML checkboxes sending a single string instead of a sequence when only one checkbox is checked
- **Mobile responsiveness** — booking rows, event type listings, and form grids now stack vertically on small screens; reduced padding on mobile; cancel form input uses responsive width

### Improved

- **Human-friendly booking dates** — dashboard bookings now show "Today at 2:30 PM — 3:00 PM", "Tomorrow at 10:00 AM — 10:30 AM", "Wednesday at 3:00 PM", etc. instead of raw ISO8601 timestamps

## [0.17.1] - 2026-03-11

### Improved

- **Host identity on booking pages** — slots page now shows host avatar, name, and title above the event type card (individual bookings only; group/team links show host name in the meta line)
- **Team link member search** — replaced checkbox list with a search bar + pill selection UX; type to filter users by name or email, click to add as a pill, remove with X; avatars shown in search results
- **Matrix-style initials** — avatar fallback now uses two-letter initials (first letter of first name + first letter of last name, e.g. "OL" for Olivier Lambert) across sidebar, settings, public profile, and booking pages
- **Onboarding hero block** — dashboard overview shows a prominent CTA card when no calendar sources exist, guiding users to add their first source

### Fixed

- Team link form validation errors now re-fetch the user list instead of showing an empty form

## [0.17.0] - 2026-03-11

### Added

- **Sidebar navigation** — persistent left sidebar on all authenticated pages
  - Organized nav sections: Scheduling (Overview, Event Types, Bookings, Team Links), Calendars (Sources), Personal (Profile & Settings, Troubleshoot), Admin (admin-only)
  - Active page highlighting with accent color
  - User avatar (with initials fallback), name, and title in sidebar header
  - Mobile responsive: hamburger menu with overlay at <768px
  - Sign out button at sidebar bottom

- **User profile** — avatar, title, and bio fields
  - Avatar upload (max 2MB, stored in `{data_dir}/avatars/`), served via `/avatar/{user_id}`
  - Title and bio fields on the settings page
  - Avatar, title, and bio displayed on public profile pages (`/u/{username}`)
  - OIDC title sync: `title` JWT claim extracted and synced on SSO login
  - New migration: `title`, `bio`, `avatar_path` columns on `users` table

- **Split dashboard** — monolithic dashboard replaced with focused pages
  - `/dashboard` — Overview with quick stats (event types, upcoming bookings, pending, sources)
  - `/dashboard/event-types` — Personal and group event types with create/edit/toggle/delete
  - `/dashboard/bookings` — Pending approval and upcoming bookings
  - `/dashboard/sources` — Calendar sources with sync/test/remove/write-back
  - `/dashboard/team-links` — Team links with copy link/view/delete
  - All sub-pages (event type form, source form, troubleshoot, admin, etc.) now render with sidebar

- **Ad-hoc team links** — create shareable booking links across hand-picked calrs users, without needing admin-managed groups
  - Pick any combination of calrs users as team members from the dashboard
  - Slot availability requires ALL selected members to be free simultaneously
  - Configurable duration, buffer times, minimum notice, and availability window (days + hours)
  - Public booking URL at `/t/{token}` — no authentication required for guests
  - CalDAV write-back to every member's calendar on booking
  - Email notifications sent to all members and the guest
  - One-time use: link auto-deleted after a successful booking
  - Team links section on the dashboard with copy link, view, and delete actions
  - `BusySource::Team` variant in the availability engine (ALL must be free, vs Group's ANY)
  - `fetch_busy_times_for_user` updated to include team link bookings
  - New migration: `team_links`, `team_link_members`, `team_link_bookings` tables
  - 2 new tests for Team intersection semantics

## [0.13.0] - 2026-03-11

### Added

- **Booking reminders** — automated email reminders sent to both guest and host before upcoming meetings
  - Configurable per event type: no reminder, 1 hour, 4 hours, 1 day, or 2 days before
  - Default for new event types: 1 day before
  - Background task runs every 60 seconds inside `calrs serve`, no external cron needed
  - Guest reminder includes a "Cancel booking" button (if `CALRS_BASE_URL` is set)
  - Host reminder includes guest name and meeting details
  - `reminder_sent_at` tracked on each booking to prevent duplicate sends
  - Catches up on missed reminders after server restart
  - Blue accent color (#3b82f6) to distinguish from confirmation (green) and cancellation (red) emails

## [0.12.0] - 2026-03-11

### Added

- **Guest self-cancellation** — guests can cancel their own bookings via a token-based link, without logging in
  - New `GET/POST /booking/cancel/{cancel_token}` public endpoints (same pattern as approve/decline)
  - Cancel form shows booking details and an optional reason textarea
  - On cancellation: booking status set to `cancelled`, CalDAV event deleted, both guest and host notified by email
  - Confirmation and pending emails now include a "Cancel booking" button linking to the cancel page
  - Requires `CALRS_BASE_URL` environment variable to generate cancel URLs
  - Graceful handling of already-cancelled, declined, or invalid tokens

### Fixed

- **Cancellation email attribution** — when the host cancels a booking from the dashboard, the host notification email no longer incorrectly says the guest cancelled; emails now correctly attribute who initiated the cancellation

## [0.11.0] - 2026-03-10

### Added

- **Per-event-type calendar selection** — choose which calendars block availability for each event type
  - New "Calendars" section on the event type form with checkboxes for all `is_busy=1` calendars
  - Junction table `event_type_calendars` links event types to selected calendars
  - If no calendars are selected, all busy calendars are checked (fully backward-compatible)
  - Filter applied across all availability paths: web slot picker, booking validation, group scheduling, troubleshoot page, and CLI commands
  - Cascade delete: removing a calendar source automatically cleans up junction rows

## [0.10.0] - 2026-03-10

### Added

- **AES-256-GCM encryption for stored credentials** — CalDAV and SMTP passwords encrypted at rest
  - Secret key auto-generated at `$DATA_DIR/secret.key` or provided via `CALRS_SECRET_KEY` env var
  - Legacy hex-encoded passwords auto-migrated on startup
  - Hidden password input via `rpassword`

## [0.9.0] - 2026-03-10

### Added

- **Timezone-aware CalDAV event handling** — CalDAV events now carry their original timezone through sync, storage, and availability computation
  - New `extract_vevent_tzid()` extracts TZID from iCal DTSTART/DTEND lines (e.g., `DTSTART;TZID=Europe/Paris:...` → `Europe/Paris`, trailing `Z` → `UTC`, no TZID → floating/local)
  - New `convert_event_to_tz()` converts event times from their stored timezone to the host's timezone before busy-time overlap checks
  - `events.timezone` column (already existed but was never populated) is now set during both CLI and web sync
  - All availability computations (slot picker, booking conflict checks, group scheduling, troubleshoot timeline) convert event times to the host's timezone
  - Pre-existing events with `timezone = NULL` are treated as floating (host-local) — fully backward-compatible
  - Invalid or unrecognized TZID strings gracefully degrade to floating (no conversion)
  - All-day events pass through unchanged (no timezone applies)
  - RRULE expansion still happens in the event's own timezone, conversion applied after — correct across DST transitions

### Fixed

- **Cross-timezone availability miscalculation** — an event at 10:00 America/New_York now correctly blocks 16:00 for a Europe/Paris host, instead of incorrectly blocking 10:00

## [0.8.5] - 2026-03-09

### Added

- **Email approve/decline for pending bookings** — host notification emails now include "Approve" and "Decline" buttons that work without logging in
  - Token-based authentication via `confirm_token` on each booking
  - Approve: confirms the booking, pushes to CalDAV, sends guest confirmation email
  - Decline: shows a form for an optional reason, notifies the guest by email
  - Requires `CALRS_BASE_URL` environment variable to generate action URLs
  - Graceful handling of already-processed bookings (already approved, declined, or cancelled)

## [0.8.4] - 2026-03-09

### Fixed

- **Multi-VEVENT CalDAV sync** — recurring events with modified instances (RECURRENCE-ID) are now split and stored as separate rows during sync, so modified occurrences correctly block or free availability
  - BlueMind bundles the parent VEVENT (with RRULE) and modified instances in a single CalDAV resource; the sync now splits them using `split_vevents()`
  - New unique index `(uid, COALESCE(recurrence_id, ''))` allows parent and modified instances to coexist
  - Fixed both CLI sync and **web dashboard sync** (which was still using the old single-VEVENT logic)
  - Migration 009 was not registered in `db.rs` — now properly included

## [0.8.3] - 2026-03-09

### Changed

- **HTML email notifications** — all booking emails now use a clean, responsive HTML design with a plain text fallback
  - Color-coded accent bar: green (confirmed), amber (pending/approval), red (cancelled)
  - Structured detail table with event, date, time, guest/host info
  - Proper HTML escaping for all user-supplied values
  - `MultiPart::alternative` ensures clients without HTML support get the plain text version

## [0.8.1] - 2026-03-09

### Fixed

- **User-scoped availability on public pages** — public booking pages (`/u/{username}/{slug}` and legacy `/:slug`) incorrectly used global busy times (all users' events) instead of the host user's events only, causing other users' calendar events to block the host's available slots
- **Group slot fallback** — the group event type slot page fallback also used global busy times; now correctly scoped to the event type owner
- **Removed dead code** — `fetch_busy_times_global()` removed since all paths now use `fetch_busy_times_for_user()`



### Added

- **Admin impersonation** — admins can impersonate any user from the admin dashboard to troubleshoot their configuration (Closes #7)
  - "Impersonate" button on each user row in the admin dashboard
  - Amber banner at the top of all pages while impersonating, showing who is being impersonated
  - "Stop impersonating" button to return to the admin's own session
  - Cookie-based implementation (`calrs_impersonate`), 24-hour expiry, HttpOnly + Secure
  - Admin pages remain accessible while impersonating (uses real session, not impersonated user)
  - Dashboard shows the impersonated user's event types, bookings, and calendar sources

## [0.7.2] - 2026-03-09

### Changed

- **Internal refactoring** — extracted shared busy-time helpers (`fetch_busy_times_global`, `fetch_busy_times_for_user`, `has_conflict`, `BusySource`) eliminating ~300 lines of duplicated availability queries across booking handlers, slot computation, and group scheduling
- **Unified slot computation** — `compute_slots` now serves both individual and group event types via a `BusySource` enum, replacing the separate `compute_group_slots` function
- **Extracted `prompt()` utility** — consolidated 4 duplicate CLI prompt functions into `src/utils.rs`

## [0.7.1] - 2026-03-09

### Fixed

- **RECURRENCE-ID handling** — modified instances of recurring events (e.g., a single occurrence moved to a different time) are now properly excluded from RRULE expansion, preventing phantom duplicate occurrences in availability checks
- **Daily COUNT bug** — `FREQ=DAILY` events with `COUNT` now correctly count all occurrences from the event start, not just those within the query window; previously a COUNT-limited daily event could produce more total occurrences than intended
- **Cancelled events ignored** — events with `STATUS:CANCELLED` in the CalDAV calendar are now excluded from all availability checks (previously they still blocked time slots)
- **RECURRENCE-ID stored during sync** — the `recurrence_id` field is now extracted from iCal data and stored in the events table (migration 008)

## [0.7.0] - 2026-03-09

### Added

- **RRULE expansion** — recurring calendar events now correctly block booking availability
  - Supports FREQ=DAILY, FREQ=WEEKLY (with BYDAY), FREQ=MONTHLY (with Nth weekday BYDAY like 2MO, -1FR)
  - Handles INTERVAL, UNTIL, COUNT, and EXDATE
  - Integrated across all availability checks: public slot picker, CLI slots, booking creation validation, troubleshoot page, and group member availability
- **Availability troubleshoot page** — visual timeline at `/dashboard/troubleshoot` showing why slots are available or blocked
  - Color-coded blocks: green (available), red (calendar event), orange (booking), gray (outside hours), striped (buffer/min notice)
  - Blocked slots breakdown with event names and calendar sources
  - Event type and date selector with prev/next day navigation

### Fixed

- **Recurring events with compact date format** — events stored in iCal compact format (`YYYYMMDDTHHMMSS`) were not found by queries comparing against ISO format (`YYYY-MM-DDTHH:MM:SS`) due to string comparison; now queries compare against both formats
- **Duplicate guest emails** — guest confirmation emails used `METHOD:REQUEST` in the `.ics` attachment, causing mail servers like BlueMind to send an additional calendar invitation; changed to `METHOD:PUBLISH` (Closes #6)
- **Missing availability rules message** — troubleshoot page now shows "No availability rules for this day" instead of the misleading "All times are bookable" when no rules exist for the selected weekday

## [0.6.1] - 2026-03-09

### Security

- **Login rate limiting** — 10 attempts per IP per 15-minute window, using `X-Forwarded-For` from reverse proxy
- **Secure cookie flag** — all session and OIDC cookies now include `Secure` (HTTPS-only)
- **ICS injection protection** — user-supplied values in `.ics` invites are sanitized (CR/LF stripped, special chars escaped per RFC 5545)
- **Security documentation** — new `docs/src/security.md` covering all security measures and known limitations

## [0.6.0] - 2026-03-09

### Added

- **CalDAV write-back** — confirmed bookings are automatically pushed to the host's CalDAV calendar via PUT, and deleted on cancellation via DELETE
  - New `put_event()` and `delete_event()` methods on the CalDAV client
  - Per-source "Write bookings to" calendar selector on the dashboard
  - Bookings track which calendar they were pushed to (`caldav_calendar_href`) for accurate deletion
  - Works for individual bookings, group round-robin bookings, and pending-then-confirmed bookings
  - No configuration needed if you don't want write-back — skipped silently when no write calendar is set

## [0.5.1] - 2026-03-09

### Added

- **Dockerfile** — multi-stage build (rust:bookworm builder, debian:bookworm-slim runtime), runs as unprivileged `calrs` user
- **`.dockerignore`** — keeps build context clean
- **systemd service file** (`calrs.service`) — production-ready unit with `ProtectSystem=strict`, `NoNewPrivileges`, and other hardening directives
- **Install section in README** — Docker, Docker Compose, binary + systemd, and from-source instructions

## [0.5.0] - 2026-03-09

### Added

- **Calendar source management from the web dashboard** — add, test, sync, and remove CalDAV sources without the CLI
  - Provider selector with presets: BlueMind, Nextcloud, Fastmail, iCloud, Google, Zimbra, SOGo, Radicale
  - Auto-fills CalDAV URL and display name when selecting a provider
  - Contextual help per provider (app passwords, skip-test tips, URL patterns)
  - Connection test before saving (with "skip test" option for tricky servers)
  - One-click sync from the dashboard (full CalDAV discovery + event fetch)
  - Connection test button to verify credentials
  - Remove with confirmation dialog (cascade-deletes calendars and events)
- **Dashboard "Calendar sources" card** — lists all connected sources with URL, username, last sync time, and action buttons

## [0.4.0] - 2026-03-09

### Added

- **Group event types** — create event types owned by a group (synced from Keycloak)
  - Combined availability: slot picker shows times where any group member is free
  - Round-robin assignment: bookings assigned to the least-busy available member
  - Public group pages at `/g/{group-slug}` and `/g/{group-slug}/{slug}`
  - Group selector when creating event types from the dashboard
- **Timezone support** — guest timezone picker on slot pages
  - Browser timezone auto-detected via `Intl.DateTimeFormat`
  - Times displayed and booked in the guest's selected timezone
  - Timezone preserved across navigation (week picker, booking form)
- Project logo

## [0.3.2] - 2026-03-09

### Added

- **OIDC group sync** — groups synced from Keycloak `groups` JWT claim on each SSO login
- **Groups in admin dashboard** — group names, member counts, and per-user group badges
- Leading `/` stripped from Keycloak group paths for cleaner display

## [0.3.1] - 2026-03-09

### Added

- **OIDC authentication** — OpenID Connect SSO via Keycloak (authorization code flow with PKCE, auto-discovery, user linking by email, auto-registration)
- **Admin dashboard** at `/dashboard/admin` — user management (promote/demote, enable/disable), auth settings (registration, domain restrictions), OIDC config, SMTP status
- **Event type management UI** — create/edit event types from the web dashboard with availability schedule, location, and confirmation toggle
- **Location support** — video link, phone, in-person, or custom location on event types; displayed on public pages, emails, and `.ics` invites
- **OIDC CLI configuration** — `calrs config oidc` with interactive and flag-based modes

### Fixed

- Multiple `Set-Cookie` headers in OIDC flow (using `HeaderMap::append` instead of array tuples)

## [0.3.0] - 2026-03-09

### Added

- **Local authentication** — email/password login with Argon2 hashing, server-side sessions (30-day TTL, HttpOnly cookies)
- **User roles** — admin/user with extractors (`AuthUser`, `AdminUser`)
- **User management CLI** — `calrs user create/list/promote/demote/set-password`
- **Registration controls** — `calrs config auth` to enable/disable registration and restrict by email domain
- **User-scoped URLs** — public booking pages at `/u/{username}/{slug}`, profile pages at `/u/{username}`
- **Booking cancellation** — cancel from dashboard with optional reason, email notifications with `.ics` METHOD:CANCEL
- **Pending bookings** — event types with `requires_confirmation`; host approves/declines from dashboard
- **Web dashboard** — event types, pending approvals, upcoming bookings

## [0.2.0] - 2026-03-09

### Added

- **Web booking page** — `calrs serve` starts an Axum HTTP server with a full booking flow:
  - `GET /:slug` — public page showing available time slots for an event type
  - `GET /:slug/book?date=&time=` — booking form with name, email, and notes
  - `POST /:slug/book` — submits the booking with conflict detection, min-notice validation, and email notifications
  - Confirmation page with booking summary
- **`calrs serve [--port 3000]`** — new CLI command to start the web server
- **Minijinja templates** — clean, responsive HTML templates (base, slots, book, confirmed) with no JavaScript dependencies
- **Server-side slot computation** — reuses the same availability engine as the CLI (availability rules, buffer times, busy events, confirmed bookings)

## [0.1.3] - 2026-03-09

### Added

- **Email notifications on booking** — when a booking is created, both the guest and the host receive an email with a `.ics` calendar invite attached (METHOD:REQUEST)
- **SMTP configuration** (`calrs config smtp`) — configure SMTP server, credentials, and sender identity. Stored in SQLite
- **`calrs config show`** — display current SMTP configuration
- **`calrs config smtp-test <email>`** — send a test email to verify SMTP setup
- **`smtp_config` table** — new migration table for SMTP settings (one per account)

### Notes

- If no SMTP is configured, bookings still work — emails are simply skipped
- Tested with Scaleway Transactional Email (SWG) on port 2525 with STARTTLS

## [0.1.2] - 2026-03-09

### Added

- **`calrs booking create <slug>`** — book a slot with full validation: minimum notice, availability rules, conflict detection against both calendar events and existing bookings
- **Booking conflict detection in slots** — `calrs event-type slots` now excludes times blocked by confirmed bookings (not just calendar events)
- **README: "Connecting your calendar" section** — CalDAV URL reference table for Nextcloud, BlueMind, Fastmail, iCloud, Google, Zimbra, SOGo, Radicale with examples

### Fixed

- **Availability engine date comparison** — properly parse iCal compact dates (`YYYYMMDDTHHMMSS`) and ISO dates (`YYYY-MM-DDTHH:MM:SS`) into `NaiveDateTime` for accurate conflict detection, instead of broken string comparison across formats

## [0.1.1] - 2026-03-09

### Fixed

- **CalDAV discovery** — proper two-step discovery: principal URL → calendar-home-set → calendar listing. Previously grabbed the first `<d:href>` instead of the one inside `<d:current-user-principal>`
- **Calendar filtering** — only sync actual `<cal:calendar/>` collections, skip inbox, outbox, notifications, freebusy, and task lists
- **URL resolution** — absolute paths from the server (e.g. `/dav/calendars/...`) are now resolved against the server origin, not appended to the base URL (which caused doubled paths like `/dav/dav`)
- **iCal date parsing** — extract DTSTART/DTEND from the VEVENT block only, ignoring VTIMEZONE entries that produced incorrect 1970 dates
- **Date format handling** — calendar show now handles both `YYYYMMDD` (iCal all-day) and `YYYY-MM-DDTHH:MM:SS` formats, with proper display formatting
- **XML tag parsing** — handle tags with attributes (e.g. `<aic:calendar-color symbolic-color="custom">`) and BlueMind-specific namespace prefixes (`aic:`, `cso:`)

### Added

- **`--no-test` flag** on `calrs source add` to skip the OPTIONS connection test (needed for servers like BlueMind that don't respond to OPTIONS)
- **10-second HTTP timeout** on all CalDAV requests (60s for event fetches) to prevent infinite hangs
- **calendar-home-set discovery** step in CalDAV client (`discover_calendar_home()`)

### Tested

- Successfully syncs with **BlueMind** CalDAV (4332 events, all-day and timed)

## [0.1.0] - 2026-03-09

Initial development release. CLI-only, no web interface yet.

### Added

- **Account setup** (`calrs init`) — interactive first-time configuration with name, email, and timezone
- **CalDAV source management** (`calrs source add/list/remove/test`) — connect CalDAV servers (Nextcloud, Fastmail, iCloud, etc.), test connections, hex-encoded credential storage
- **Calendar sync** (`calrs sync`) — pull events from all CalDAV sources via PROPFIND/REPORT, upsert into local SQLite
- **Calendar viewer** (`calrs calendar show`) — display synced events in a table with date range filtering
- **Event types** (`calrs event-type create/list/slots`) — define bookable meeting templates with duration, buffers, and minimum notice. Default Mon–Fri 09:00–17:00 availability rules
- **Availability engine** — compute free slots by intersecting availability rules with synced busy events
- **Booking management** (`calrs booking list/cancel`) — view and cancel bookings
- **SQLite storage** — WAL mode, foreign keys with CASCADE, indexed queries
- **CalDAV client** — minimal RFC 4791 implementation: OPTIONS check, principal discovery, calendar listing, VEVENT fetch
