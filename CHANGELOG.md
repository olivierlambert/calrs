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
| Dynamic group links | 1.2.0 | Ad-hoc collective meetings via `/u/alice+bob/slug` — no team setup needed |
| OIDC team member roles | 1.2.0 | Set admin/member role on OIDC-synced team members without permission sync |
| SOGo CalDAV compatibility | 1.2.0 | Handle arbitrary XML namespace prefixes in CalDAV parser |
| Default availability | 1.3.0 | Per-user working hours used by dynamic group links to constrain participants |
| Deferred slot loading | 1.3.0 | Instant page render with async slot computation for dynamic group links |
| Force full resync | 1.3.0 | Dashboard button + automatic 24h periodic full resync to catch deleted events |
| Booking watchers | 1.4.0 | Designate team members as watchers on event types — they're notified of new bookings and can claim them |
| Event type form redesign | 1.4.0 | Collapsible sections with progressive disclosure, dynamic recaps, pre-filled availability defaults |
| Collective member exclusion | 1.4.0 | Opt specific members out of collective team event types while keeping them on the team |
| Security review hardening | 1.4.0 | 7 findings from third-party security review addressed |
| Configurable slot interval | 1.5.0 | Slot start-time spacing decoupled from event duration (e.g., 20-min meetings on 30-min boundaries) |
| Security audit round 2 | 1.6.0 | 3 findings from @marcotama's third-party audit addressed (OIDC email-based account linking, stored XSS in onclick handlers, CSRF Secure flag) |
| Explicit event-type timezone | 1.7.0 | Availability rules now pinned to a chosen IANA timezone per event type (previously derived from the creator's profile) |
| Cross-timezone team availability | 1.7.0 | Team slot grid respects each member's personal working hours converted from their own timezone into the event's host timezone |
| Multi-language UI | 1.8.0 | Public booking flow + 3 highest-volume guest emails translated, six locales shipped (English, French, Spanish, Polish, German, Italian) with Fluent + Hosted Weblate |
| Per-user language preference | 1.8.0 | Logged-in users can pick a UI language in Profile & Settings; guests get browser detection (RFC 7231 with q-weights) |
| Locale-aware date formatting | 1.8.0 | Month and weekday names + per-locale date format patterns rendered server-side, no more chrono `%B %A` English-only formats |
| Bulk private invites | 1.9.0 | Paste a list of emails (one per line, max 100) on the invite page; each row becomes its own single-use invite token with a shared optional message |
| Copy-link button on invites | 1.9.0 | Each active sent invite has a "Copy link" button next to Delete to retrieve the URL after the fact, useful when SMTP delivery fails or you need to re-share |
| Security audit round 3 | 1.10.0 | One High and seven Mediums from @marcotama's third-party audit addressed (login timing oracle, OIDC client_secret encryption, X-Forwarded-For trust, company_link XSS, error sanitization, atomic first-admin, OsRng for sessions, constant-time CSRF) |
| Guest cancel/reschedule notice window | 1.10.0 | Per-event-type minimum lead time before a guest can self-cancel or self-reschedule via the tokenized email links; host actions from the dashboard are unaffected |
| Estonian locale | 1.10.0 | First community-language slot added beyond the original four (English, French, Spanish, Polish, German, Italian) |
| Admin user deletion | 1.10.0 | Admins can permanently delete users from the admin panel with cascade rules and confirmation |
| Capped slots hidden in picker | 1.11.0 | When a frequency limit has been hit for the day/week/month/year, the slot picker skips those times instead of letting a guest pick a doomed slot |
| Per-member booking frequency limits | 1.11.0 | Opt-in "Per team member" flag on each frequency-limit row so caps apply to individual round-robin members (e.g. 1 demo/day per person) instead of pooled team-wide |
| Edit CalDAV sources | 1.12.0 | Fix a typo'd URL, change a username, or rotate the password from `/dashboard/sources/{id}/edit` or `calrs source update <id-prefix>`. Empty password preserves the existing one; URL changes still pass the SSRF validator |
| SMTP via environment variables | 1.12.0 | Configure SMTP via `CALRS_SMTP_*` env vars instead of the database, useful for container deployments. Env vars take priority over the DB; partial config errors loudly |
| SMTP implicit TLS (port 465) | 1.12.0 | New `tls_mode` column (`starttls`/`tls`) supports port 465 without the prior STARTTLS hang. Applies to both env-var and DB-configured SMTP |
| Team event type permission cleanup | 1.12.0 | Personal event-type mutation routes can't be reached via team URLs and vice versa; centralised through `can_manage_event_type` / `find_manageable_event_type_by_slug` with 8 new regression tests covering the manageability matrix |
| Microsoft Exchange (EWS) backend | 1.13.0 | On-prem Exchange 2013/2016/2019 via a minimal SOAP client behind the new `CalendarProvider` trait, with autodiscover and protocol-filtered presets |
| Google Calendar (OAuth2) sources | 1.13.0 | Connect Google Calendar without an app password: admin-configured OAuth2 client, tokens encrypted at rest, proactive refresh |
| Auto-generated meeting links | 1.13.0 | Jitsi room URLs built from pattern tokens per confirmed booking, or a webhook location type for bring-your-own providers |
| Booking captcha | 1.13.0 | Opt-in self-hosted proof-of-work captcha (Cap) on booking pages, no third-party tracking, CSP rebuilt at runtime |
| Embed system | 1.13.0 | `embed.js` with inline auto-sizing iframe, floating button, and element-click bindings; layout/theme/brand params |
| `calrs config dump` | 1.13.0 | Full instance configuration as JSON (18 sections), secrets excluded by construction |
| Private-host allowlist | 1.13.0 | `CALRS_ALLOW_PRIVATE_HOSTS` lets listed CalDAV/EWS hosts resolve to private IPs while the SSRF guard stays on for everything else |
| Runtime SMTP configuration | 1.14.0 | Configure and test SMTP from the admin panel; env vars keep precedence, password encrypted at rest |
| Runtime base URL + allowlist | 1.14.0 | Public base URL and private-host allowlist editable from the admin panel instead of env-only |
| Shared bookable resources | 1.15.0 | Instance-level resources (demo lab, meeting rooms) backed by an ICS feed: busy resources block slots, reservations written back over CalDAV |
| Per-resource team allowlists | 1.15.0 | Team admins attach allowlisted resources to their own team event types, enforced server-side |
| SMS notifications | 1.16.0 | Opt-in guest text messages on confirm/cancel/reschedule/reminder, provider-agnostic across Twilio, GatewayAPI, seven.io and any webhook gateway, with a daily spend cap |
| Guest phone country picker | 1.16.0 | Flag picker, format-as-you-type and libphonenumber validation on the booking form, vendored and served same-origin so the page makes no third-party request |
| Rolling booking horizon | 1.16.0 | Per-event-type limit on how many days ahead a guest may book (NULL = unlimited) |
| Guest "Add to calendar" | 1.16.0 | The confirmation page serves the booking as `.ics`, so a guest can add it even on an instance with no SMTP |
| Brazilian Portuguese locale | 1.16.0 | Eighth shipped language, contributed by @RafaelGrochoska |
| Unauthenticated SMTP relays | 1.16.0 | Leave the username empty to relay through a local MTA; new `none` TLS mode for a relay with no STARTTLS or a private-CA certificate |
| Localized dashboard | 1.17.0 | The host side — dashboard, settings, every form, admin panel, sign-in — renders through Fluent, so an operator can run calrs in their own language |
| Eight complete locales | 1.17.0 | English, French, German, Spanish, Italian, Polish, Brazilian Portuguese and Estonian at 100%, held there by a test |
| Google Meet auto-links | Unreleased | Host-owned Google Meet conference per confirmed booking, using existing Google Calendar OAuth2 tokens (#45 phase 3) |

## [Unreleased]

### Added

- **Google Meet auto-generated links** (issue #45 phase 3) - Event types can use location `google_meet`. On confirmation, calrs attaches a Meet conference to the host's Google Calendar event via Calendar API `conferenceData` (same OAuth tokens as Google CalDAV, no extra scope). The Meet URL flows through emails, ICS, and other calendar write-backs. Team event types require every eligible member to have Google Calendar connected with write-back before the location can be saved. Round-robin assigns Meet to `assigned_user_id`; collective uses the same host as ORGANIZER.

### Fixed

- **Approval-path meeting host** - Dashboard and email-token approval now pass `COALESCE(assigned_user_id, owner)` into meeting URL generation, so a team admin approving a round-robin booking no longer stamps their own username into a Jitsi room (or owns the Google Meet) instead of the assigned member.

## [1.17.1] - 2026-08-30

Patch release. 1.17.0 said the booking flow's bare error responses were localized. Most were; the page they actually render was not. A guest who mistyped an email address, followed an expired cancellation link, or opened an already-approved booking got English regardless of the language they had been booking in. This finishes that, and fixes five paths that answered with no page at all. No migrations, no configuration change.

### Fixed

- **The booking error page spoke English to every guest** - Two mechanisms fed it: twenty-one call sites passing an English sentence, and eight handlers rendering the template inline with their own hard-coded pair. Both are gone; every one now names a message id resolved against the guest's `Accept-Language`. The same applies to the form validators, which returned a sentence and now return an id. Forty-three keys, translated across all eight locales, which stand at 1043.
- **Five booking paths answered a date too far out with an unstyled fragment** - The date check returned its message as a bare `Html`, so instead of the error page a guest got a wall of unformatted text with no header, no theme and no way back. They render the real page now, in the guest's language.
- **A dynamic-group booking announced a member conflict in English** - `/u/alice+bob/intro` reported "This slot is no longer available (alice has a conflict)" as a raw string. Its own neighbours in the same handler had been translating the equivalent message since 1.17.0; this one had been missed. It also covers the guest-count cap, which now carries a plural selector, so Polish reads correctly at two and at five where a single form cannot.

### Internal

- Three guard tests, each mutation-checked, and two of them only because the check found them broken first. The plural test passed with a locale's selector deliberately flattened, because comparing the two renderings is satisfied by the interpolated numeral alone; normalising the digit away catches it. The key guard could not see ids that reach Fluent through a variable rather than inline, which is exactly the shape this release introduces, so a typo would have rendered the raw id to a guest with nothing failing. The third asserts no call site hands the error page a bare English sentence.
- One definition of how calrs renders an integer. Rust call sites built their own arguments with the library default, which groups thousands; the template path had always switched grouping off. Both go through `i18n::number()` now, on values too small for the difference to have shown.
- An end-to-end test posts a booking with a bad email under `Accept-Language: fr` and asserts the French is on the page, the English is not, and the raw id is not. The static guards cannot see through the three steps between a validator and a rendered page.
- 913 tests, all green.

## [1.17.0] - 2026-08-30

Minor release. The headline is that **calrs speaks your language on both sides of the booking link**: the host-facing interface is localized, and all eight shipped locales are complete rather than partial. No migrations, no configuration change, and no visible difference for English users.

### Added

- **The host-facing interface is localized** (closes #195, PR #196, reported by @typovrak) - The dashboard, settings, every form, the availability troubleshooter, date overrides, invite management, the admin panel and the sign-in and registration pages now render through Fluent. Before this, someone could offer guests a booking page in their own language but had to administer it in English. The language resolves once in the `AuthUser` / `AdminUser` / `OptionalAuthUser` extractors, where the user row and the request headers are both already in hand — saved preference first, then `Accept-Language` — so the language dropdown that shipped in #59 as "foundation for the dashboard translation pass" finally does something. Validation errors and the bare responses the booking flow returns without page chrome are localized too, including the ones the guest side had never covered despite #195 assuming otherwise.
- **All eight locales complete** (PR #197) - German, Spanish, Italian, Polish and Brazilian Portuguese each gained 821 keys, Estonian all 987 from an empty file, and every locale now carries 1003. Each follows the register its earliest translations chose — du, tu, tú, ty, você, sa — rather than a formal one, and matches the English voice on the details that usually drift: the same ellipsis character per string, the same softening on validation errors, and one rendering per English sentence where the same sentence appears under two keys. Polish carries the three plural categories its grammar needs rather than English's two, so "2 członkowie" and "5 członków" are both right. Native review is still welcome on Weblate, Estonian most of all.

### Fixed

- **A host could inject markup into a guest-facing page through their own booking address** - The "booking can no longer be cancelled" page put the host's email into a `mailto:` href and into the link text through a filter that turns off escaping, so an address containing quotes and angle brackets reached the guest's browser as markup. Found by a structural test added while localizing: in any translated sentence that carries markup, every value spliced in must be escaped, and the rule now fails the build rather than relying on review.
- **French phone hint agreed with the wrong gender** - `{ $country } est supposé` cannot agree with an interpolated country name — *la France est supposée*, *le Portugal est supposé*. Rephrased to avoid agreeing with the placeholder, as the other six locales already did.

### Internal

- Six guard tests, each mutation-checked: every `t()` key referenced from a template or from Rust exists in English; every template still loads; every locale covers every English key; plural messages carry the categories their language needs; every host-facing page passes a `lang` into its render context; and no value is spliced into a markup-bearing translation without escaping. The last two exist because both failures shipped inside the branch and neither made anything look broken — a page in the wrong language still renders, and an unescaped value still displays.
- `t()` hands integers to Fluent as numbers rather than strings, so plural selectors select. A stringified `1` never matches the `one` variant and falls through to `other`, which is how "1 members" reaches a page. Grouping is off, so an integer still renders as it always did — 1440, not 1,440.
- A test race is fixed (PR #198). `config_general_set_and_clear` failed intermittently because `caldav`'s allowlist test set `CALRS_ALLOW_PRIVATE_HOSTS` process-wide from a module that `commands::config`'s own lock could not reach. The lock and its restore guard moved to `crate::test_support` so every module shares them; measured at 5 failures in 12 runs before and 0 in 12 after.
- 910 tests, all green.

## [1.16.0] - 2026-08-21

Minor release. The headline is **SMS notifications** — opt-in, off by default twice over, and provider-agnostic — alongside a rolling booking horizon, a Brazilian Portuguese locale, and a batch of field-reported fixes. Two migrations (`062`, `063`), both additive; existing deployments upgrade in place with no configuration change.

### Added

- **SMS notifications** (closes #130, PR #184) - Optional text messages to the guest on four events: confirmed (or approved, for event types that require confirmation), cancelled, rescheduled, and the reminder. `SmsProvider` mirrors the `CalendarProvider` trait, with four adapters shipping — Twilio, GatewayAPI, seven.io, and a generic HMAC-signed webhook so a gateway with no adapter is a small script away. Each adapter parses its own vendor's failures onto a shared error type, because "the credentials are wrong" is an HTTP 401 on Twilio, a 401 with a body code on GatewayAPI, and an HTTP **200** carrying `{"success": "900"}` on seven.io. One admin card drives all of them from a spec registry, and "Test gateway" verifies credentials for free where the vendor has a check endpoint. Bodies are composed from Fluent keys in the booking's own language, with a test asserting every shipped body in every shipped language stays inside two GSM-7 segments. Off by default: with no `sms_config` row and every event type at `sms_phone_mode = off`, nothing in `src/sms/` ever runs. Idea and field testing by @Chr1s16 (#180).
- **Spend controls on SMS** - A public booking form that spends real money on a recipient the guest chooses is the SMS pumping (AIT) attack, so three layers guard it: `sms_allow_all_users` (off by default) keeps enabling SMS an admin decision, `sms_config.daily_cap` stops sending past a segment budget while email carries on regardless, and the admin card warns when SMS is configured and the captcha is not — that combination being the actual open relay. `sms_usage` records segments and cost and deliberately stores no recipient number.
- **Country picker on the guest phone field** (PR #188) - intl-tel-input 25.3.1, vendored and served from `/static/`, so a booking page makes no third-party request; the 265 KB libphonenumber bundle is lazy-loaded rather than linked on every page. The country is **seeded, not guessed**: only an explicit BCP-47 region subtag counts, otherwise the SMS default country code. A bare language tag is ignored on purpose, because a language is not a country — `sv` reads as El Salvador, `pt` would send Brazilian guests to Portugal. With JavaScript off the field still posts and is still normalised server-side.
- **Per-event-type rolling booking horizon** (PR #186, contributed by @iamthew4lrus789) - Cap how many days into the future a guest may book. Empty means unlimited, which is the existing behaviour, so no event type changes on upgrade.
- **Guest "Add to calendar"** - `GET /booking/ics/{cancel_token}` serves the booking as `text/calendar` and the confirmation page links it. The confirmation email already attached one, but the guest is looking at the page at the moment they want to add it — and an instance with no SMTP never sends that email at all.
- **Copy button on event types** (PR #183, contributed by @gsmachado) - One click copies a bookable link from the dashboard listing, with a fallback for browsers without the Clipboard API. Shown only where a public URL actually resolves; a private event type still points at "Send invites".
- **Brazilian Portuguese** (PR #176, contributed by @RafaelGrochoska) - Eighth shipped language, complete.
- **Authentik OIDC setup guide** (PR #145, contributed by @arthur-perrot) - Full walkthrough plus the `email_verified` claim change in Authentik 2025.10 that otherwise blocks every login with a generic failure.

### Fixed

- **SMTP relays that advertise no AUTH** (closes #190, PR #191, reported by @ManUtopiK) - `send_email()` attached credentials unconditionally, so a configured-but-empty username still put lettre into the authenticating path; it gates on presence, not emptiness. A local MTA relaying anonymously from the loopback advertises no mechanism, and every send aborted client-side before the envelope reached the server. Configurable through both the CLI and the admin form, and it simply could never send. Credentials are now attached only when the username is non-empty. Two adjacent gaps closed with it, since neither left a working path for that deployment: the `CALRS_SMTP_*` block no longer requires `USERNAME`/`PASSWORD`, and a new `tls_mode` of `none` reaches a relay with no STARTTLS or a private-CA certificate — calrs validates against the compiled-in Mozilla roots, not the system trust store, so those were unreachable in every prior mode.
- **Guest names with a semicolon no longer break CalDAV write-back** (closes #163, PR #192) - `CN=` carried a value escaped as if it were a TEXT value, so a semicolon came out as `\;`. A parameter takes no backslash escapes (RFC 5545 §3.1), the raw `;` stayed in place, and a strict server — Yandex 360 in the report — answered `400 Bad Request` to the whole VEVENT. Because a failed write-back is only logged, the guest saw "You're booked!" while nothing reached the calendar. Parameters are now quoted, with RFC 6868 caret escapes for what a quoted-string cannot hold. The cancellation ICS had the same bug, so an affected guest could not have been un-booked either.
- **OAuth2 CalDAV sources are editable again** (closes #121, PR #143, contributed by @bboles) - Clicking Edit on a Google source rendered an empty basic-auth form whose submit crashed in `decrypt_password` on the NULL password. OAuth2 sources now show **Reconnect**, which re-runs the consent flow, and both handlers early-return for `auth_type = 'oauth2'` so the crash is unreachable by direct URL too. The PR also fixed an adjacent bug it uncovered: reconnecting an already-linked account refreshes the existing source's tokens instead of creating a duplicate source.
- **OIDC ID tokens with additional audiences** (PR #144, contributed by @flokflok1) - Providers that mint a token whose `aud` carries more than the client ID were rejected at login.
- **Team reschedule availability** (closes #166, PR #167, contributed by @hugo-fasone) - A reschedule checked availability against the event type owner rather than the booking's actual host, so a round-robin booking could be moved to a time the assigned member was not free. Completes the fix started in #160.
- **Real dark palettes for custom themes** (PR #178) - A custom accent colour produced a dark mode derived from the light palette, which came out washed out or illegible depending on the colour.
- **Slot picker edge cases** (PR #187) - The back arrow rendered the month before the first bookable one as the literal string "none", and a negative booking horizon failed open (every slot bookable) instead of closed.
- **Runtime settings the environment is forcing** (closes #142, PR #192) - The admin panel skipped writing an env-forced field; the CLI wrote it anyway and printed a note. Nothing broke at the time, because the env keeps precedence — the footgun is later, when removing the variable silently activates a value stored for another machine. For the private-host allowlist that is a change to the SSRF posture with no action at that moment. The CLI now refuses the write and says so.

### Internal

- CI runs on pull requests targeting the `i18n` branch (PR #189). CLAUDE.md tells contributors to branch off `i18n` for anything touching UI text, which had made those exactly the PRs with no automated checks.
- The `cargo-tarpaulin` guard on the tracing-capture tests is keyed to `cfg!(tarpaulin)` rather than an env var name that moves between releases (closes #110).
- 900 tests, all green.

## [1.15.1] - 2026-08-02

Patch release with three field-reported fixes on top of 1.15.0. No schema changes, no new features, no config changes.

### Fixed

- **Team reschedules reach the calendar again** (closes #159, PR #160, contributed by @hugo-fasone) - Rescheduling a booking on a team event type updated the database but pushed an ICS whose `ORGANIZER` was the event type owner, while the event had been created with the assigned member as organizer. Google CalDAV rejects an organizer change on an existing event with `403 Forbidden`, so the calendar silently kept the old time while guest and host were told the reschedule succeeded. The new `booking_host_identity` helper resolves the organizer from the booking itself (assigned member for round-robin, full roster for collective, owner for personal event types) and is applied to the guest reschedule, token approval, and dashboard approval paths. Follow-up on the availability side tracked in #166.
- **No more black frame around themed embeds** (PR #165, contributed by @pycanis) - An inline embed pinned to `?theme=light` showed its light content inside a black frame for visitors whose OS is in dark mode (and vice versa): the `color-scheme` meta always advertised `light dark`, so the iframe canvas followed the OS instead of the pinned theme. The meta now follows the `?theme=` embed param; auto-themed embeds and regular pages are unchanged.
- **Neutral subjects on guest emails carrying a calendar invite** (closes #157, PR #158) - Exchange iMIP processing titles the guest's calendar appointment after the email Subject rather than the ICS SUMMARY, so prefixes like "Confirmed:" leaked into the event title and bounced back in accept/decline replies. The three guest-facing emails that attach a `METHOD:REQUEST` ICS now use a plain "event, date" subject in all six locales; the confirmed/rescheduled wording stays in the email body.

### Internal

- 797 tests, all green (new regression test covering organizer resolution for assigned, collective, and personal bookings)

## [1.15.0] - 2026-07-25

The shared-resources release: attach instance-level bookable resources (a demo lab, meeting rooms) to your event types. A busy resource blocks the slot, confirmed bookings are reserved in the resource's own CalDAV calendar, and team admins manage attachments through per-resource allowlists. Plus a round of team booking fixes reported from the field.

### Added

- **Shared bookable resources** (#149, #152) - Resources are backed by a read-only ICS publish feed (BlueMind, Nextcloud, any iCalendar URL) and managed from the admin panel: feed validation, "Sync now", "Test write", sync-failure indicator. Two modes per event type: **all** (every resource must be free) and **round-robin** (least-loaded free resource is picked and shown on the booking). Reservation write-back into the resource's CalDAV collection prevents cross-event-type double-booking; reservations are released on cancel and reschedule
- **Per-resource team allowlists** (#153) - Team admins attach allowlisted resources to their own team event types, enforced server-side
- **CLI parity for resources** (#150) - `calrs event-type slots` consults resources, `calrs booking cancel` releases reservations and cleans the host calendar
- New docs page: [Shared Resources](https://olivierlambert.github.io/calrs/docs/resources.html)

### Fixed

- **Write-back targets the right calendar** (#147) - Team bookings land on the assigned member's calendar, not the event type owner's. Collective mode is now fully implemented at booking time: it verifies every member, pushes to every member's calendar, and notifies every member. Reminders and the sync orphan sweep follow the same resolution
- **Per-member slot capacity** (#146) - The double-booking guard is keyed per assigned member, so a round-robin team can take as many parallel bookings per slot as it has free members
- **No more duplicate Fastmail invites** (#141, PR #155) - CalDAV write-back marks attendees `SCHEDULE-AGENT=CLIENT` (RFC 6638), so scheduling-aware servers no longer send their own UTC invitation on top of the calrs email

### Internal

- Migrations 058-061 (`resources`, `resource_sync_error`, `resource_teams`, `booking_unique_per_member`), applied automatically on startup
- Thanks to Michael (3dreams-medienagentur) and Matthijs (@MatthijsZw) for the detailed reports and reproducible setups

## [1.14.0] - 2026-06-22

Configuration that used to require environment variables and a restart moves into the admin UI and the database. Environment variables still take precedence when set, so existing deployments are unaffected.

### Added

- **Editable SMTP config from the admin UI** (#139) - Configure and test your SMTP server from the panel, with explicit env/DB precedence. The password is encrypted at rest (AES-256-GCM) and uses the keep-current pattern on save
- **Configurable base URL and private-host allowlist** (#140) - Set the public base URL and the private-host allowlist at runtime via DB/UI. Env vars still win when present, and the SSRF guard stays on by default (allowlist empty unless explicitly configured by an admin)

### Fixed

- Same-day calendar events are now detected in the busy-time check (#134)
- Radicale calendars advertised with a spaced self-closing tag are now discovered (#135)

### Internal

- Migration 057 (`runtime_settings`), applied automatically on startup
- 766 tests, all green

## [1.13.0] - 2026-06-06

The provider-expansion release: connecting a calendar no longer means CalDAV with basic auth. Microsoft Exchange (EWS) joins as a second backend behind a new provider trait, Google Calendar connects via OAuth2 with encrypted token storage, confirmed bookings can auto-generate video meeting links (Jitsi pattern or bring-your-own webhook), booking pages gain an opt-in self-hosted proof-of-work captcha, and an embed system lets you put your booking page on any website. Two headline features are community contributions.

### Added

- **Microsoft Exchange (EWS) backend** (PR #103) - On-prem Exchange 2013/2016/2019 support via a minimal SOAP client in `src/ews/` (autodiscover, GetFolder/FindItem/CreateItem/DeleteItem, iCal to EWS field mapping). A new provider factory in `src/providers/` abstracts CalDAV vs EWS behind a single `CalendarProvider` trait; the CalDAV path is unchanged. Source-add form gains a Backend dropdown with protocol-filtered presets
- **Google Calendar (OAuth2) sources** (PR #99) - Admin configures the OAuth2 client ID/secret in the admin panel; users connect via "Add Google Calendar" on the sources page or `calrs source add-google`. Access and refresh tokens are stored AES-256-GCM encrypted and refreshed proactively five minutes before expiry
- **Auto-generated video meeting links** (PR #128, phases 1+2 of #45) - New `Jitsi (auto-generated room)` location type: every confirmed booking gets a fresh URL built from pattern tokens (`{username}`, `{event}`, `{date}`, `{random}`; default `{event}-{random}`), configurable org-wide and overridable per event type. Plus a `Webhook (custom provider)` location type for bring-your-own providers: calrs POSTs booking details and uses the returned URL
- **Booking captcha** (PR #122, contributed by @florian-SV) - Opt-in [Cap](https://trycap.dev) integration: self-hosted proof-of-work captcha on booking pages, no third-party tracking. Without configuration the booking flow is byte-for-byte unchanged. The Cap secret is encrypted at rest, and the CSP is rebuilt in memory on every admin save, scoped to booking form pages only
- **Embed code generator** (PR #125) - Cal.com-style embed system: a self-contained `embed.js` exposes `Calrs.inline` (auto-sizing iframe), `Calrs.floatingButton` (corner pill + modal), and `Calrs.elementClick` (data-attribute binding). `?embed=1` strips navigation chrome, autosizes via `calrs:resize` postMessages, accepts layout/theme/brand params, and switches the CSRF cookie to `SameSite=None; Secure` so cross-origin iframes work
- **`calrs config dump`** (PR #112, contributed by @mvalois) - Dumps the full instance configuration as JSON (`--pretty` supported): 18 sections plus a top-level `schema_version`. Secrets and operational sync state are excluded by construction, with tests asserting they never appear
- **`CALRS_ALLOW_PRIVATE_HOSTS`** (PR #124, closes #123) - Opt-in, comma-separated, exact-match allowlist letting specific CalDAV/EWS hosts resolve to private/reserved IPs (docker-compose stacks, self-hosted Exchange behind private addressing). The SSRF guard stays active for every non-listed host. Reported and verified by @aburg

### Fixed

- **Google sync silently truncating the forward window** (PR #99) - An unfiltered `calendar-query` REPORT left future events out of the local cache, making booked days look available. The full-fetch path now sends an RFC 4791 `time-range` filter with a 90-day lookback, falling back to the unfiltered REPORT for servers that reject it; orphan cleanup is scoped to the same window so pre-window history is preserved
- **CalDAV write-back no longer gated on SMTP** (PR #99) - All four guest booking handlers previously wrapped the CalDAV push inside an SMTP-config check, so bookings on instances without SMTP never reached the host's calendar. Confirmed bookings now push unconditionally; only email sends remain gated on SMTP. A dashboard warning surfaces when sources are enabled but none have a write target
- **EWS: timed UTC events, all-day date offset, recurring series** (PR #127) - Follow-up correctness pass on the EWS backend
- **Friendly email validation on booking forms** (PR #129) - Incomplete addresses like `user@domain` (no TLD) pass HTML5 validation but failed server-side with a bare error page; guests now get a proper inline error

### Internal

- Migrations 053-056 (`oauth2_caldav`, `captcha`, `provider_type`, `meeting_links`)
- Translation updates from Weblate (PR #131)
- 758 tests, all green (up from 677 in 1.12.1)

## [1.12.1] - 2026-05-26

Patch release fixing wall-clock display in host-targeted emails. A Paris organizer reading a cancellation email for a Los Angeles guest's 07:00 booking would see "07:00" with no timezone label and naturally read it as Paris time (the correct Paris-local time is 16:00). No schema changes, no behaviour changes outside the email rendering surface.

### Fixed

- **Host emails display the host's wall-clock with a TZ label** (closes #119, PR #120) — `send_host_notification`, `send_host_booking_confirmed`, `send_host_reminder`, `send_host_cancellation`, `send_host_approval_request`, and `send_host_reschedule_request` now convert times into the host timezone (event-type tz via `get_host_tz`, falling back to `users.timezone`) and append a `(TZ)` suffix, e.g. `16:00 – 16:30 (Europe/Paris)`. Guest cancellation and decline emails gained the TZ label they were missing (confirmation and pending notices already had it). New `convert_time_between_tz` helper handles date rollover (e.g. LA 22:00 → Paris 07:00 next day) and DST gaps.
- **Reminder loop and reschedule "Previous" times stopped double-converting** (PR #120) — `run_reminder_loop` and `guest_reschedule_booking`'s `old_*_time` extraction pulled `start_at` raw from the DB (event-type-tz wall-clock since #101) but labeled it with `guest_timezone`. With the new host-tz conversion path, this would have shifted the displayed time twice — a 16:00 Paris booking would render as "01:00 next day" in the host reminder. Both paths now run through `booking_strings_in_guest_tz` first, matching the contract used by the cancel/confirm/decline handlers. As a bonus this also fixes a pre-existing latent bug where the guest reminder was showing host-tz wall-clock under a guest-tz label.
- **`host_reschedule_booking` switched to `get_host_tz`** (PR #120) — was using `user.timezone` directly, inconsistent with the other 15 call sites. SELECT now includes `et.id`; resolved host timezone derives from the event type.

### Internal

- 677 tests total (up from 671 in 1.12.0), all green on pre-commit
- New helpers in `src/email.rs`: `convert_time_between_tz`, `host_time_display` (both unit-tested across DST, date-rollover, invalid-zone, and same-tz cases)
- `BookingDetails`, `CancellationDetails`, `RescheduleDetails` gain a `host_timezone: String` field

## [1.12.0] - 2026-05-24

A modernization pass on operator-facing surfaces: source connection details are now editable instead of delete-and-readd; SMTP gains env-var configuration and proper port-465 support; the From: mailbox stops mangling addresses without display names; and team event-type management permissions are tightened so management routes can't be reached via the read-only availability surface.

### Added

- **Edit CalDAV sources** (closes #72, PR #73) — `GET /dashboard/sources/{id}/edit` (form pre-filled, password field reads "Leave blank to keep existing") and `POST /dashboard/sources/{id}/edit` (apply), both scoped by `user_id`. CLI mirror: `calrs source update <id-prefix> [--name ...] [--url ...] [--username ...] [--password]`. `--password` is a flag that prompts via `rpassword` for scripted rotation. Empty password on either surface preserves the stored encrypted blob untouched. URL changes pass `validate_caldav_url` for SSRF parity between web and CLI
- **SMTP via environment variables with TLS mode support** (PR #56) — `CALRS_SMTP_HOST`, `CALRS_SMTP_USERNAME`, `CALRS_SMTP_PASSWORD`, `CALRS_SMTP_FROM_EMAIL` required; `CALRS_SMTP_PORT` (default 587), `CALRS_SMTP_TLS_MODE` (`starttls`/`tls`), `CALRS_SMTP_FROM_NAME` optional. Env vars take priority over the DB; partial config errors loudly. New `SmtpTlsMode` enum branches `send_email` on `relay()` (implicit TLS) vs `starttls_relay()` to fix the port-465 hang. Applies to DB-configured SMTP too via new `tls_mode` column (migration 052, defaults to `'starttls'`); `calrs config smtp` prompt asks for the mode. Admin panel surfaces env-sourced status. `SmtpConfig`'s `Debug` impl now redacts the password so it can't leak through tracing or test output

### Fixed

- **From: mailbox handles missing display name and special characters** (PR #104) — `calrs config smtp-test` produced `Error: Invalid input` whenever `from_name` was `None`: the old fallback yielded a string like `you@example.com <you@example.com>` whose two `@`'s failed RFC 5322 parsing. All 17 send sites now build From through a new `SmtpConfig::mailbox_from()` helper using `Mailbox::new(Option<String>, Address)`, which also handles display names containing commas and other characters that need quoting. Unit test locks in both the `None` case and the `Some("Name, With Comma")` case
- **Team event type management gated through a single capability check** (PR #55) — personal `/dashboard/event-types/{slug}/*` mutation routes now require `et.team_id IS NULL`; team-event mutation requires team admin; slug-collision resolution made deterministic via subquery + `ORDER BY (team_id IS NULL) DESC`. **Behaviour change worth noting:** `delete_invite` now strictly requires `can_manage_event_type`, so a non-admin team member who created an internal-event invite via the "any authenticated user" path can no longer delete that invite (let it expire or hit max-uses; owners, team admins, and global admins are unaffected). New `OptionalAuthUser` extractor with shared `resolve_session_user` lets public surfaces selectively widen access for logged-in viewers. Team members and global admins can now bypass the team-level invite token on public events of private teams; `team_profile_page` lists private/internal event-type titles + slugs for logged-in team members (booking stays invite-gated). 8 new regression tests cover the full manageability matrix

### Internal

- 671 tests total (up from 650 in 1.11.0), all green on pre-commit
- Migration 052 (`smtp_config.tls_mode`)

## [1.11.0] - 2026-05-22

Two themes: closing the 1.10.2 sync-robustness hotfix loop (the three follow-ups filed against #105 / #106 / #107) and turning the booking-frequency-limits surface from a half-wired feature into a real one — first by hiding capped slots in the picker, then by adding per-team-member caps.

### Added

- **Hide booking slots when a frequency cap is reached** (closes #115, PR #116) — `compute_slots` now runs `apply_frequency_limit_filter` after slot generation: for each configured `(max, period)` it counts existing confirmed+pending bookings per containing host-local period, then drops slots that fall in any capped period. The submit-time check (`would_exceed_frequency_limit`) stays as a race-condition backstop, but the picker no longer shows times the submit-time check would reject. Limit-reached page also got proper styling (template render instead of bare `Html("...")`)
- **Per-member booking frequency limits** (closes #117, PR #118) — new `booking_frequency_limits.per_member` flag (migration 051) so a host can express "1 demo/day per team member" instead of "1 demo/day team-wide". Threaded through three sites: `would_exceed_frequency_limit` takes `Option<&str> assigned_user_id` and scopes the count by assignee; `pick_group_member` excludes candidates already at their per-member cap so the picker doesn't route to a doomed user; `apply_frequency_limit_filter` hides a slot only when every eligible team member is at cap. UI gets a "Per team member" checkbox on each limit row, hidden on personal event types

### Fixed

- **CalDAV resource is HEAD-checked before cancelling a booking** (closes #105, PR #108) — sync-collection's "deleted" entries are now treated as a hint, not a verdict: before cancelling a confirmed booking we HEAD the resource href and only act if the server confirms 404. Two new regression tests (one for the BlueMind phantom-deletion case, one for the legitimate deletion case)
- **`cancel_orphaned_booking` is scoped to its own source/account** (closes #106, PR #111) — previously did a global UID-only lookup against the bookings table, so a sync running on source A could cancel a booking whose CalDAV event lived under source B (different calendar, different account). Now joins through `event_types → accounts → caldav_sources` and only acts on rows whose source matches the one currently syncing
- **Property-level 404s inside `<d:propstat>` are ignored** (closes #107, PR #109) — the sync-collection parser previously treated *any* 404 status code inside a `<d:response>` as a deletion, including the per-property 404 some servers emit when one of the requested DAV properties is absent on an otherwise-live resource. Parser now distinguishes resource-level status from propstat-level status and only routes resource-level 404s into the deletion handler
- **Team event type frequency limits actually persist** (PR #114) — `edit_group_event_type_form` never queried `booking_frequency_limits`, and `create_group_event_type` / `update_group_event_type` never wrote to it. Toggling the cap on a team event type was a silent no-op. Three-way fix mirrors the working personal-event-type flow

### Internal

- 650 tests total (up from 634 in 1.10.2), all green on pre-commit
- Migration 051 (`booking_frequency_limits.per_member`)
- `render_claim_error` renamed to `render_booking_action_error` since the template (`booking_action_error.html`) isn't claim-specific
- Coverage CI: `under_tarpaulin()` now uses `cfg!(tarpaulin)` (the previous `CARGO_TARPAULIN_VERSION` env-var check never fired, so the racy tracing-capture tests were leaking through the supposed-to-skip guard); `Cargo.toml` registers `cfg(tarpaulin)` via the `unexpected_cfgs` lint config

## [1.10.2] - 2026-05-14

Hotfix for a production incident in which CalDAV sync was wrongly cancelling live customer bookings. No schema changes, no behaviour changes outside the orphan-cancellation surface.

### Fixed

- **Stop cancelling bookings on phantom sync-collection "deletions"** — `delete_events_by_href` (src/commands/sync.rs:443) cancelled any booking whose UID matched an href the server reported as deleted, regardless of whether the local `events` table had a matching row. In production, BlueMind's sync-collection emitted a "deleted" entry for an href whose local event lived on a different calendar (the booking's write calendar); `cancel_orphaned_booking`'s global UID-only lookup still found the confirmed booking and cancelled it + emailed host and guest "the event was deleted by the host." The cancellation is now gated on `rows_affected > 0` — if we never had a local event for that calendar/href, that's a server-side false positive and we log a warning instead. Two regression tests added: one captures the exact prod failure mode, the other guards the legitimate deletion case from regressing

### Internal

- 634 tests total (up from 632 in 1.10.1), all green on pre-commit
- Three follow-ups tracked separately for the next release: confirm-before-cancel via HEAD on the resource href (#105), source/account scoping in `cancel_orphaned_booking` (#106), propstat-aware sync-collection XML parser (#107)

## [1.10.1] - 2026-05-12

Patch release fixing booking-time timezone display across the dashboard, the post-booking emails, and the ICS attachments they carry. No schema changes, no behaviour changes outside the timezone-display surface.

### Fixed

- **Dashboard booking times now render in the host's profile timezone** (closes #100) — listings previously used the raw naive datetime stored against the event-type tz plus the server's local clock for the "Today"/"Tomorrow" label, with no tz suffix. With an event type configured in `America/New_York` and a host in `Europe/Paris`, the dashboard showed `10:00` with no qualifier and let the host believe the meeting was at 10:00 Paris time. The dashboard now reads `et.timezone` (with fallback to `users.timezone`) as the stored tz, converts to the host's profile tz, and appends a tz abbreviation. When the guest's selected tz differs, a muted secondary line shows the guest's view too so the host can see both
- **Confirmation, decline, and cancellation emails (and their ICS attachments) now use the guest's timezone** (closes #101) — five post-booking action handlers (dashboard "Approve", email-link "Approve", dashboard "Cancel"/"Decline", email-link "Decline", guest self-cancel link) fed the event-type-local stored datetime straight into `BookingDetails`/`CancellationDetails`. Because `email::generate_ics`/`generate_cancel_ics` both call `convert_to_utc(date, start_time, end_time, guest_timezone)`, the email body, the rendered post-action page, *and* the ICS UTC came out wrong: a booking made at 16:00 Paris on a New York event type arrived in the guest's inbox as 10:00, and the CalDAV write-back landed at a different absolute time than the host saw on the dashboard. New `booking_strings_in_guest_tz()` helper converts stored times through the event-type tz to the guest tz before populating either struct. Regression tests cover both the approve and the cancel surface

### Internal

- 632 tests total (up from 624 in 1.10.0), all green on pre-commit

## [1.10.0] - 2026-05-04

Security audit round 3 (one High, seven Mediums) plus a guest-side cancel/reschedule notice window. Also bundles two months of translation work merged from the long-lived `i18n` branch and a few self-hoster fixes that landed since 1.9.0.

### Security

All eight items below were originally reported by @marcotama in a third-party audit; the High was fixed by the audit author themselves, the seven Mediums were addressed in this release.

- **High — Login timing oracle leaked user existence** (#77, fixed by @marcotama) — `login_handler` short-circuited to "Invalid email or password" before running Argon2 if the email wasn't registered, leaking a ~10ms vs ~microseconds gap that was usable to enumerate registered emails over the network. Fixed by always running Argon2 against a static `DUMMY_HASH` (Argon2id with `Argon2::default()` parameters) when the user is missing or has no password set, so all three branches (user found + correct, user found + wrong, user not found) take the same time
- **Medium — OIDC client_secret stored in plaintext** (#94) — CalDAV and SMTP credentials were already AES-256-GCM encrypted, but the OIDC client secret in `auth_config` sat alongside them as plaintext. New `crypto::encrypt_value` / `decrypt_value` API with an `enc:v1:` sentinel prefix unambiguously distinguishes encrypted values from plaintext at migration time (the existing base64 envelope can collide with plaintext OIDC secrets that happen to look like base64). Existing plaintext values are transparently re-encrypted on next startup; the migration is idempotent and uses try-decrypt as a belt-and-suspenders against the prefix-collision edge case
- **Medium — Rate limiter trusted leftmost X-Forwarded-For** (#90) — all six rate-limited handlers extracted the *leftmost* XFF value, which is exactly the attacker-controlled one (each proxy in the chain *appends* to the right). Rotating the header per request bypassed per-IP rate limits entirely. Consolidated the six copy-pasted extractions into `client_ip_for_rate_limit()` and switched to the rightmost value (the trusted proxy's view of its peer). `X-Real-IP` is intentionally not honoured because neither default Caddy nor default Nginx overwrite a client-supplied value, so trusting it would have been a worse footgun
- **Medium — Stored XSS via `company_link` `javascript:` scheme** (#93) — the admin-controlled company link is rendered as a clickable anchor on every public booking page; an admin (or attacker who took over an admin account) could set `javascript:alert(1)` and land arbitrary script on every visitor. New `is_safe_company_link()` allowlists `http(s)://` only and is enforced on both write (admin handler returns the user to `/dashboard/admin?error=...`) and read (silently drops bad values as defense in depth)
- **Medium — Internal errors leaked to clients** (#91) — template render, database, and OIDC errors were `format!`'d straight into HTTP response bodies, leaking template paths, schema hints, IdP URLs, and occasionally token contents. ~144 sites in `src/auth.rs` and `src/web/mod.rs` now route through one of `internal_error_response` / `internal_error_html` / `internal_error_body`, all of which log the underlying detail via `tracing::error!` and return a generic message. OIDC has its own `oidc_error_response` with auth-flow-specific text. Operator-facing CalDAV source-test/sync feedback is intentionally preserved, since the only viewer is the admin debugging their own configuration
- **Medium — TOCTOU race in first-admin role assignment** (#89) — three sites (registration handler, OIDC auto-register, CLI `user create`) computed the first-user-is-admin role with a separate `has_any_users()` SELECT before the INSERT, letting two concurrent registrations both observe an empty users table and both claim admin on a fresh DB. All three sites now compute the role atomically inside the INSERT via `CASE WHEN NOT EXISTS (SELECT 1 FROM users) THEN 'admin' ELSE 'user' END`. Extracted `auth::create_local_user` so the web and CLI paths share one helper and the test exercises the production code path
- **Medium — Session tokens used userspace `thread_rng`** (#86) — `generate_session_token` used `rand::thread_rng()` (a userspace ChaCha12 PRNG) for 30-day session secrets while `crypto.rs` already uses `OsRng` (kernel CSPRNG via `getrandom`) for AES-GCM keys/nonces. Switched to `OsRng.fill_bytes`, matching the existing pattern. Output shape unchanged (32 bytes hex-encoded → 64 chars)
- **Medium — CSRF token comparison was not constant-time** (#87) — `verify_csrf_token` used `String == String`, which short-circuits on the first differing byte. Replaced with `subtle::ConstantTimeEq::ct_eq` on the underlying byte slices. Risk in practice was low (network jitter dwarfs the leaked timing and CSRF tokens are UUID v4) but the fix is one extra direct dependency on a crate that was already transitively pulled in via `argon2`

The remaining Lows and Informationals from the same audit are tracked in issue #85 as a punch list.

### Added

- **Minimum notice for guest cancel and reschedule** (closes #95) — two new optional `event_types` columns (`cancel_notice_min`, `reschedule_notice_min`); `NULL` or `0` keeps the previous behaviour of allowing cancel/reschedule at any time. Within the configured window, the four guest token endpoints (`/booking/cancel/{token}` and `/booking/reschedule/{token}`, GET + POST) render a friendly `booking_action_blocked` page showing the host's contact email instead of mutating booking state. Host- and admin-initiated cancellations from the dashboard are unaffected, since hosts often need to act on real-world emergencies on behalf of a guest. Policy is also surfaced inline on the confirmed page and in the localized confirmation email body so guests aren't surprised at click time. Form fields use a numeric input + minutes/hours/days unit selector
- **Admin user deletion** (#70) — admins can permanently delete users from the admin panel with cascade rules and a confirmation prompt. Self-delete and last-admin delete are blocked; users with future bookings as host are blocked unless they are deleted via the dashboard with explicit acknowledgement
- **Estonian locale (`et`)** — first community-language slot beyond the original four. Stub file is empty (runtime falls back to English on missing keys); new keys are added to `i18n/en/main.ftl` only and Weblate picks them up at the next sync

### Changed

- **Clippy on tests in CI** (#75) — `cargo clippy --all-targets -- -D warnings` now also covers test code, catching a class of regressions the previous `clippy` step missed

### Fixed

- **Event-type availability defaults respect the user's profile** (closes #68, #69) — newly-created event types now seed their availability rules from the creator's per-user default working hours rather than a blanket Mon-Fri 9-17 fallback when the form is submitted without explicit windows
- **CalDAV connection check falls back to PROPFIND when OPTIONS doesn't advertise `calendar-access`** (#71) — some CalDAV servers (notably some SOGo deployments) don't advertise `calendar-access` in the `DAV:` OPTIONS response header even though they support the protocol; calrs now retries with a PROPFIND probe before giving up, fixing connection-test failures for those backends
- Various i18n context-plumbing fixes in `user_profile`, public profile, settings, footer, and booking pages (translations rolled in via the `i18n → main` merge)

### Internal

- 624 tests total (up from 575 in 1.9.0), all green on pre-commit
- `crypto::encrypt_value` / `decrypt_value` introduced for fields where stored values can ambiguously look like plaintext; future credential additions should use these instead of `encrypt_password` directly when migration disambiguation matters
- New `client_ip_for_rate_limit()`, `internal_error_response()` (+ `_html` / `_body`), `oidc_error_response()`, `is_safe_company_link()`, `auth::create_local_user()`, `check_notice_window()` helpers consolidate copy-pasted handler code
- Two months of community translation work merged from the long-lived `i18n` branch (the standard `i18n → main` direction; `main → i18n` remains an explicit anti-pattern)

## [1.9.0] - 2026-04-28

Workflow improvements on the invite page and a self-hoster bug fix that affected anyone running calrs without SMTP configured.

### Added

- **Bulk private invites** (closes #58) — the per-recipient invite form is replaced with a paste textarea (one email per line, capped at 100). Each row becomes its own single-use invite token, with a shared optional message and the existing expires/single-use settings. The result page summarizes counts of sent, invalid, duplicate, and failed rows
- **Copy-link button on each active sent invite** — surfaces the invite URL through the UI so it can be re-shared via Slack, a separate email client, or any out-of-band channel. URL is pre-computed server-side using `CALRS_BASE_URL` and the existing team/user route patterns. Hidden on expired and used invites since the link is no longer actionable

### Fixed

- **Auto-confirm bookings now write back to CalDAV regardless of SMTP availability** (closes #65) — across the four booking-creation handlers (`handle_booking_for_user`, `handle_group_booking`, `handle_dynamic_group_booking`, `handle_booking`), `caldav_push_booking()` was nested inside the `if let Ok(Some(smtp_config)) = ...` block. When SMTP was not configured the entire block was skipped, taking the CalDAV write-back down with it. The host-approval path was already correct, which is why **require-confirmation** bookings showed in CalDAV but **auto-confirm** bookings silently didn't. Affected anyone who deployed calrs and tried it before configuring SMTP, which is most first-time self-hosters. `BookingDetails` construction (and the host-info lookup it depends on) is now hoisted out of the SMTP gate, with `caldav_push_booking` and `notify_watchers` running as siblings of the SMTP block instead of children. `notify_watchers` already self-gated on SMTP for its email part, so its behaviour is unchanged

### Internal

- 575 tests total (up from 569 in 1.8.0), all green on pre-commit

## [1.8.0] - 2026-04-26

Internationalization release: the public booking flow and the highest-volume guest emails (confirmation, reminder, cancellation) are translated. Six locales ship out of the box, with English as the source, French human-translated by maintainers, and Spanish / Polish / German / Italian AI-seeded as starting points for native-speaker refinement on Hosted Weblate.

### Added

- **Six-language UI** — public booking flow (slot picker, booking form, confirmation, cancel/decline/approve/claim/reschedule pages, theme-toggle chrome) renders in English, French, Spanish, Polish, German, or Italian. Translations are stored in [Fluent](https://projectfluent.org/) `.ftl` files under `i18n/{lang}/main.ftl`, embedded into the binary at compile time via `include_str!`. The single-binary deploy story is preserved
- **Automatic language detection** — guests get their browser's `Accept-Language` (RFC 7231 with q-weights honoured); logged-in users can override via a Language dropdown in **Profile & Settings** (migration `047_user_language` adds `users.language TEXT`)
- **Translated guest emails** — confirmation, reminder, and cancellation emails render in the language captured at booking time. Migration `048_booking_language` adds `bookings.language TEXT`, populated from the booking POST handler. Reminder background task pulls the column at send time so reminders fired days later still use the right language
- **Server-side date localization** — `format_month_year` and `format_long_date` helpers render dates using locale-specific patterns: English `Tuesday, March 12, 2026`, French `mardi 12 mars 2026`, Spanish `martes, 12 de marzo de 2026`, German `Montag, 12. März 2026`, Italian `lunedì 12 marzo 2026`. Format pattern itself is a Fluent message, so word order is a translation choice
- **Title-cased month header** — calendar header CSS-capitalizes the first letter (`avril 2026` → `Avril 2026` visually) without touching the underlying lowercase Fluent values that mid-sentence date labels need
- **Hosted Weblate integration** — translators contribute via [hosted.weblate.org/projects/calrs](https://hosted.weblate.org/projects/calrs/) without git or Rust knowledge; commits flow back to the long-lived `i18n` branch automatically via the Weblate GitHub App
- **Translation-quality table in README** — explicitly distinguishes human-translated French from AI-seeded locales and points readers at Weblate as the contribution path

### Fixed

- **Docker image build broken since 1.8.0 i18n scaffolding was introduced** — the multi-stage Dockerfile didn't `COPY i18n/`, so `include_str!` on the embedded `.ftl` files failed at release-image build time even though local `cargo build` worked (it reads source directly). One-line fix added in time for this release

### Internal

- Migrations `047_user_language` and `048_booking_language` (test count assertion bumped to 48)
- New `src/i18n.rs`: concurrent `FluentBundle` per locale in `OnceLock`, `Accept-Language` parser with q-weight sort, minijinja `t(key, **kwargs)` global, `is_supported`/`resolve`/`supported_with_labels` helpers, and the date-formatter pair
- 30 templates and ~25 web handlers wired through, each handler computes `lang` once via `crate::i18n::detect_from_headers` and threads it into render contexts
- `BookingDetails` and `CancellationDetails` gain `guest_language` and `host_language` fields; both derive `Default` so existing call sites use `..Default::default()`. `EmailRow.label` widened from `&'static str` to `String` so labels can be translated
- 569 tests total (up from 545 in 1.7.0), including 11 new unit tests for `Accept-Language` parsing, 6 for date-formatter output across locales, and full-locale-coverage tests for Spanish, Polish, German, Italian
- Long-lived `i18n` branch documented in `CLAUDE.md` as the working branch for translator commits and new translatable-string features. Branch is permanent, never deleted on merge

### Known limitations

- Host-side emails (notification, reminder, cancellation, approval-request, decline) remain English. The infrastructure (`host_language` field, reminder bg task already loading it from `users.language`) is in place; translation pass scheduled for a follow-up
- The pending-notice, decline-notice, and reschedule guest emails are not yet translated. Same pattern, same follow-up
- `decline_booking_by_token` and dashboard-host-cancel paths don't yet load `bookings.language`; guest cancellation emails sent from those paths fall back to English
- Polish month names are nominative, so date contexts read informally (`27 kwiecień 2026` instead of grammatical `27 kwietnia 2026`). Native-speaker refinement on Weblate is welcome; could be addressed via a separate genitive-form key set
- Dashboard, admin panel, and CLI command output remain English

## [1.7.0] - 2026-04-24

Correctness and resilience release: fixes an OOM-triggering infinite loop in slot computation, adds explicit per-event-type timezones, makes team slot grids honour each member's personal working hours, and parallelizes per-member CalDAV syncs with per-source deduplication.

### Added

- **Explicit timezone on event types** (issue #50) — each event type now carries its own IANA `timezone` column; availability rules are interpreted in that timezone rather than silently inheriting the creator's profile timezone. Surfaces a timezone picker inside the Availability section of the event-type form. Migration `046_event_type_timezone` backfills every existing row with the current account owner's timezone, so upgrades preserve behaviour
- **Per-member working hours on team events** — team slot grids now intersect each member's personal `user_availability_rules` (in the member's own timezone) with the event-type's rules. Members without explicit personal hours stay unconstrained (no auto-seeded 9–17 default is planted). Prevents the scenario from issue #50 where a team event in Paris would offer bookings at 09:00 Paris to a US-based member whose real working hours are 09:00 Chicago
- **Member timezone shown on event-type priority list** — the Member Priority / Required Members section now displays each member's timezone under their name, making mis-configured user timezones immediately visible to admins

### Fixed

- **Infinite slot loop → OOM when availability window ends near midnight** — `compute_slots_from_rules` walked its inner cursor as a `NaiveTime`, and `NaiveTime + Duration` wraps at 24h. On a rule ending at 23:00 with a 60-minute slot duration, `cursor + slot_duration` wrapped to 00:00 (still ≤ 23:00 as a time-of-day), producing an infinite loop that allocated SlotTimes until the kernel OOM-killed the process. Production manifested as a ~4-minute CPU spike, ~9 GB RAM growth, ~240 GB of SQLite re-reads under memory pressure, and an OOM kill. Cursor now walks as `NaiveDateTime` so midnight rolls into the next day cleanly. Regression test pins the exact failing config (09:00–23:00, 60-min slot, 60-min interval)
- **Dashboard Decline button no-op on pending bookings** (#51) — `cancel_booking` filtered on `status = 'confirmed'`, so clicking Decline on a pending booking matched zero rows and silently redirected. Broadened to include `pending` and branches on current status: confirmed bookings are cancelled (CalDAV delete + emails), pending bookings are declined (no CalDAV delete, guest decline notice only). Mirrors the email-token decline flow

### Performance

- **Parallel per-member CalDAV sync on team/dynamic-group slot pages** — team slot pages used to sync each member's CalDAV sources sequentially, so latency was Σ(per-member sync) and a single slow server stalled the whole request. Now fans out via `tokio::task::JoinSet`, guarded by a per-source async mutex inside `sync_if_stale`: same-source concurrent calls serialize and the loser skips after re-checking staleness, so at most one CalDAV fetch per source is in flight at any time across the whole process. Memory ceiling stays bounded even under burst traffic

### Internal

- Migration `046_event_type_timezone` with backfill from the account owner's timezone
- New helper `normalize_event_type_tz` validates IANA names submitted via the form with a safe fallback
- Regression tests: `get_host_tz_prefers_explicit_event_type_timezone`, `compute_slots_terminates_with_window_ending_at_23_00`, `chicago_member_is_busy_at_paris_morning`, `member_without_personal_rules_is_unconstrained`, `source_lock_identity`, `sync_if_stale_serializes_on_per_source_lock`
- 545 tests total (up from 537 in 1.6.0), all green on pre-commit
- Verified end-to-end against a copy of a production DB: the previously-OOMing team booking URL now responds in under a second with flat RSS

## [1.6.0] - 2026-04-23

Security audit follow-ups, CalDAV compatibility with non-standard ports, ICS RFC-5545 compliance, and a reschedule-flow correctness fix.

### Security

- **OIDC account takeover via email-based linking** (audit from #43) — `find_or_create_oidc_user` fell through to matching by email without checking the ID token's `email_verified` claim, so any IdP that allows unverified registrations (Keycloak's default) let an attacker attach their `oidc_subject` to any existing local account by registering at the IdP with the target's email. Now gated on `email_verified=true` for both the email-link and auto-register branches; the `oidc_subject` match path (step 1) is unchanged so returning users keep working. Missing claim treated as `false` (conservative default)
- **Stored XSS via backslash injection in inline onclick handlers** (#43) — three dashboard templates (event types, sources, team settings) embedded user-controlled strings inside `onclick="… '\\'{{ var }}\\'…"`. MiniJinja doesn't HTML-escape backslashes, so a crafted payload (e.g. `\\'));alert(1);//`) could break out of the JS string and execute arbitrary script. Severity was HIGH because team event types and team-settings pages are multi-viewer. Fix moves the value into a `data-confirm` attribute read via `this.dataset.confirm`, eliminating the JS parsing context entirely. Thanks @marcotama for the report
- **CSRF cookie missing Secure flag** (audit from #43) — `csrf_cookie_value` lacked `Secure`, so the token would travel over plaintext HTTP on a misconfigured deployment. `HttpOnly` intentionally stays off — the double-submit pattern needs the client JS in `base.html` to read the cookie

### Fixed

- **CalDAV sync fails with non-standard port** (#42) — origin-building stripped the port, so Nextcloud-style relative hrefs like `/remote.php/dav/principals/users/alice/` got resolved against port 443 instead of the configured port (e.g. 8080). Connection-test succeeded because it hits `base_url` directly with the port intact; the sync path only broke at the second PROPFIND. BlueMind users were unaffected because it returns absolute URLs that bypass the origin resolver
- **Missing DTSTAMP in generated ICS** (#49) — required by RFC 5545 §3.6.1 for VEVENT. Strict clients (RustiCal) rejected invites; permissive ones (Gmail, Outlook) silently accepted, which is why this went undetected. Added to both `generate_ics` and `generate_cancel_ics` as the current UTC time in RFC 5545 §3.3.5 form-#2 format. Thanks @Handfish for the report and fix
- **Pending bookings auto-cancelled on reschedule approval** (#44) — sync treated the original booking slot as free after reschedule and cancelled the new booking

### Internal

- Regression tests for: DTSTAMP presence + format, onclick template interpolation across the three previously-vulnerable templates, OIDC `email_verified` gate (6 tests covering attack scenario, legitimate flow, squatting variant, returning-user bypass), lettre CRLF rejection in `Mailbox::from_str` + RFC 2047 encoding of CRLF in `.subject(…)`, CSRF cookie security flags, CalDAV URL resolution with/without port
- Empirically verified that the email-header-injection concern flagged in @marcotama's audit is a false positive — lettre's typed builder rejects or encodes all tested CRLF payloads. Pinned with regression tests rather than adding a redundant sanitizer
- 537 tests total (up from 521 in 1.5.0), all green on pre-commit

## [1.5.0] - 2026-04-22

Configurable slot interval — start-time spacing is now independent from event duration.

### Added

- **Slot interval** (#38) — new optional field on the event type form. By default, slot start times are spaced by duration (existing behaviour). Set explicitly to control the cadence: a 20-minute event with a 30-minute interval produces slots at 9:00, 9:30, 10:00, … Buffers and minimum notice still apply on top. Stored as nullable `slot_interval_min` on `event_types`; NULL inherits duration — fully backward-compatible

### Internal

- Migration `045_slot_interval.sql`
- 6 unit tests covering default, custom, and edge-case intervals (null, zero, interval > duration, buffer overlap)

## [1.4.1] - 2026-04-20

Calendar invite compatibility fixes for Gmail.

### Fixed

- **ICS invite not parsed by Gmail** (#36) — trailing whitespace in the generated `method_line`, attendee lines, and `VALARM` block leaked into the output, so `BEGIN:VEVENT` and other property lines started with spaces. Per RFC 5545 §3.1 a line starting with whitespace is a folded continuation of the previous line, so Gmail saw the whole attachment as one folded `METHOD` line and never detected a VEVENT
- **METHOD:PUBLISH on guest invites** (#36) — confirmation and reschedule emails sent the ICS with `METHOD:PUBLISH`, which clients treat as read-only informational feeds. Gmail and most clients only show the "Add to calendar" / RSVP banner for `METHOD:REQUEST`. The ICS already carried `ORGANIZER` + `ATTENDEE;RSVP=TRUE`, so REQUEST is what the content already expressed. Host notifications (already REQUEST) and cancellations (CANCEL) are unchanged

### Changed

- Guests may now see an RSVP prompt (Accept / Decline / Maybe) on booking confirmation and reschedule emails, where some clients previously showed only an informational entry

### Internal

- Regression test asserting no ICS line starts with whitespace
- Flaky-on-Monday test pattern fixed across 23 booking/slot tests: `next_monday` now starts from tomorrow so it's always strictly in the future

## [1.4.0] - 2026-04-17

Booking watchers, event type form redesign, security hardening, and a critical timezone fix for cross-zone bookings.

### Added

- **Booking watchers** (#31) — designate team members as watchers on team event types. Watchers receive booking notifications and can claim any unclaimed booking via email link, with a dashboard view of claimable bookings. Introduces `event_type_watchers` and `booking_claim_tokens` tables
- **Collective member exclusion** — opt specific members out of collective team event types from the team settings UI, without removing them from the team. Per-event-type `excluded_member_user_ids` stored alongside member weights
- **Event type form — progressive disclosure** — sections collapsed by default (only Basics stays open), with smooth open/close animations and dynamic hint recaps summarising each section's contents
- **Pre-filled availability from user defaults** — new event types start with the user's configured default working hours
- **Invite Links shortcuts** — "+ New internal event" header button and direct link to create internal event types from the Invite Links page
- **CalDAV observability** — tracing for PROPFIND and write-back requests to aid debugging

### Fixed

- **Booking flow timezone mix-up** (#33) — slot pages displayed guest-local times correctly, but the `/book` URL carried the host's local time. The booking form re-rendered that host-local time to the guest, and the ICS generator then re-interpreted it as guest-local before converting to UTC — so the confirmed meeting ended up offset by the guest↔host timezone delta. The URL now carries guest-local date/time end-to-end; the backend converts to host-local for storage and conflict checks via a `guest_to_host_local` helper
- **First-slot-only persistence** — `first_slot_only` was silently dropped on group event type update due to a missing SQL bind
- **Security review findings** — 7 issues identified during security review resolved
- **TRANSP:TRANSPARENT events** no longer consume availability (events marked as free in CalDAV)
- **Timezone picker** — defaults to Midway for unlisted IANA zones instead of crashing; mismatch banner no longer shown for equivalent timezones (e.g. `Europe/Paris` ↔ `CET`)
- **CalDAV write-back and delete** — operate on all configured sources per user, not just the first; ICS omits `METHOD` property so servers don't double-invite
- **Nextcloud CalDAV discovery** — DAV headers recognized on OPTIONS connection test
- **SoGo compatibility** — unprefixed `<calendar>` element in namespace-less CalDAV responses is correctly detected

### Changed

- **Invite Links URL** — `/dashboard/organization` renamed to `/dashboard/invite-links`
- **Event type form layout** — duration moved into the Basics card; "Scheduling" section renamed to "Availability"; location moved into Basics and made required

### Internal

- Docker builds cache Rust dependencies between layers, shaving cold-cache rebuilds
- Regression test for group event type update persistence
- Regression tests for the guest↔host timezone conversion helper

## [1.3.0] - 2026-03-25

Per-user default availability, instant page loads for dynamic group links, and sync reliability improvements.

### Added

- **Per-user default availability** — set your working hours in Profile & Settings. Used by dynamic group links to constrain participants' availability (e.g., no slots outside your 9-17 window). Auto-seeded to Mon-Fri 9:00-17:00 on first use. Timezone-aware: a New York participant's 9-17 correctly maps to 15-23 in Paris
- **Dashboard onboarding banner** — one-time notice about default availability with a direct link to Settings#availability. Dismissable
- **Deferred slot loading** — dynamic group link pages render instantly with a spinner, then fetch availability asynchronously. Prevents 10s+ waits when multiple users' CalDAV calendars need syncing
- **Force full resync button** — "Full resync" on the dashboard sources page clears sync tokens and re-fetches all events from the server, detecting deletions that delta sync missed
- **Automatic full resync every 24h** — the background sync loop forces a full resync per source once per day, catching orphaned events automatically (e.g., events deleted in BlueMind that sync-collection didn't report)

### Fixed

- Dynamic group slot page: avatars stacked vertically above names to prevent layout overflow with many participants
- Dynamic group deferred loading: `clearSlotPanel()` was destroying the slot panel title element, causing `selectDay()` to crash on the deferred callback
- Dynamic group month navigation: prev/next month fetched without `&deferred=1`, returning empty slots instead of computed data
- Deferred slot loading: show "No availability found for all participants" instead of misleading "Click a highlighted date" when no mutual slots exist
- Event type form: empty numeric fields (buffer, duration, etc.) no longer crash with "cannot parse integer from empty string"
- Participant availability timezone conversion: rules are now interpreted in each participant's timezone and converted to the host's timezone for slot computation
- Orphan detection logging: remote vs local event counts per calendar, individual UIDs being removed

## [1.2.0] - 2026-03-25

Ad-hoc collective scheduling and team permission improvements.

### Added

- **Dynamic group links** — combine usernames in a URL (`/u/alice+bob/intro`) to create instant collective meetings. The first user's event type defines settings (duration, buffers, availability rules); all participants' calendars are intersected to show only mutually available slots
  - **Opt-out setting** — users can disable being included in dynamic group links from Profile & Settings (enabled by default)
  - **Stacked avatars** — dynamic group slots page shows overlapping avatar circles for all participants
  - **Autocomplete user picker** — event type edit page has a search-as-you-type dropdown to build dynamic group URLs, filtered to users who opted in
  - **CalDAV write-back with attendees** — confirmed bookings are pushed to the owner's calendar with co-participants as ATTENDEE in the ICS; CalDAV servers propagate the invite
- **OIDC team member role management** — team admins can now set admin/member roles on OIDC group-synced members directly from the team settings UI. Locally-set roles are preserved across OIDC login sync

### Fixed

- Team role dropdown now uses a `<select>` element instead of a toggle button for clearer UX
- CalDAV XML parser handles arbitrary namespace prefixes (e.g. SOGo's non-standard prefixes) instead of requiring specific ones

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
