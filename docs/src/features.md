# Features in v2

This is the feature map for v2, including capabilities carried forward from v1. It describes implemented behavior in the v2 source, not proposed features. For changes between individual builds, see the [changelog](https://github.com/olivierlambert/calrs/blob/main/CHANGELOG.md).

## Calendars

| Capability | What you can do |
|---|---|
| CalDAV | Connect Nextcloud, BlueMind, Fastmail, iCloud, SOGo, Zimbra, Radicale, Baikal, Kerio Connect and other CalDAV servers; discover calendars, sync changes and write bookings back. [CalDAV](./caldav.md) |
| Google Calendar | Connect with OAuth2, block out-of-office events, write bookings and create host-owned Google Meet links. [Google Calendar](./google-calendar.md) |
| Microsoft 365 / Outlook.com | Connect with delegated OAuth2 through Microsoft Graph; select calendars and a writable destination. [Microsoft Calendar](./microsoft-calendar.md) |
| On-premises Exchange | Connect individual mailboxes through EWS, or provision managed calendars with a service account and trusted mailbox addresses. [Exchange](./exchange.md), [EWS impersonation](./ews-impersonation.md) |
| Calendar selection | Choose which calendars a source syncs, which synced calendars block each event type, and which calendar receives each host's confirmed bookings. [Selection and write-back](./caldav.md#choosing-which-calendars-are-synced) |
| Sync and recovery | Refresh on demand and in the background, force a full resync, inspect sync errors and write-back retries, and receive persistent-failure alerts. [Sync failures](./caldav.md#when-a-sync-fails), [background sync](./deployment.md#background-calendar-sync) |
| Recurring events | Account for recurrence rules, added/removed dates and moved instances; conservatively block unsupported rules. [Recurring events](./caldav.md#recurring-events) |

## Availability and booking rules

| Capability | What you can do |
|---|---|
| Working hours | Set default personal hours and multiple windows per day, then give each event type its own schedule and timezone. [Availability](./event-types.md#availability-schedule) |
| Time off and exceptions | Take time off across all meetings you host, or override one event type's hours on a date. [Time off](./event-types.md#time-off), [date overrides](./event-types.md#availability-overrides) |
| Timing controls | Set duration, slot interval, before/after buffers, minimum booking notice and guest cancel/reschedule notice. [Event types](./event-types.md), [notice policy](./booking-flow.md#notice-window-policy) |
| Booking windows | Set a rolling horizon or an inclusive fixed date range; expired event types leave public listings. [Booking horizon](./event-types.md#booking-horizon), [date range](./event-types.md#bookable-date-range) |
| Frequency limits | Cap bookings per day, week, month or year, optionally per round-robin member; show only the earliest remaining slot per day. [Booking limits](./event-types.md#booking-limits) |
| Approvals | Require confirmation, hold pending slots, expire unanswered requests and remind the host to answer. [Confirmation mode](./booking-flow.md#confirmation-mode) |
| Conflict checks | Re-check availability when booking and approving; account for busy calendars, bookings, time off, member hours and required resources. [Conflict detection](./booking-flow.md#conflict-detection) |
| Troubleshooting | See why a time is unavailable, including conservative blocking of unsupported recurrence rules. [Dashboard](./dashboard.md#availability-troubleshoot) |

## Meetings and sharing

| Capability | What you can do |
|---|---|
| Personal and team meetings | Book one host, assign an available team member by priority and workload, or require all eligible members. [Teams](./teams.md) |
| Dynamic groups | Combine usernames in a link such as `/u/alice+bob/intro` for an ad-hoc collective meeting. [Dynamic group links](./event-types.md#dynamic-group-links) |
| Watchers | Notify a support team and let the first person claim a booking; the confirmed claim blocks their time. [Booking watchers](./teams.md#booking-watchers) |
| Visibility and invitations | Publish meetings, let colleagues distribute internal meetings, or send private invitations; use expiry, usage limits and bulk invitations. [Event types](./event-types.md) |
| Directory and favorites | Search meetings you can share, copy public links, generate internal invites and keep favorites on your overview. [Booking directory](./dashboard.md#booking-directory-and-favorites) |
| Public pages | Add profile facts, Markdown descriptions, meeting sections and ordering; show meeting duration, location and confirmation requirements. [Sections and order](./event-types.md#sections-and-order-on-your-page) |
| Embeds | Add an inline picker, floating booking button or click-triggered modal; set layout, theme, branding, timezone and clock format. [Embedding](./embedding.md) |
| Shared resources | Reserve rooms, demo labs or other resources from ICS availability feeds, with optional CalDAV write-back and team allowlists. [Resources](./resources.md) |

## Guest experience and notifications

| Capability | What you can do |
|---|---|
| Slot picker | Offer month, week and column views, a timezone picker, 12/24-hour clocks and a configurable first weekday. [Calendar views](./event-types.md#calendar-views) |
| Booking questions | Ask short/long text, single/multiple choice and checkbox questions; record consent and retain the question as answered. [Booking questions](./booking-questions.md) |
| Attendees and notes | Allow additional guests, choose whether guest notes are optional/required/hidden, and keep separate host-only notes. [Booking flow](./booking-flow.md), [host notes](./booking-flow.md#host-notes) |
| Changes after booking | Approve, decline, cancel, reschedule or ask a guest to choose a new time; keep calendar copies and attendees updated. [Booking flow](./booking-flow.md) |
| Email | Send translated HTML/plain-text messages, reminders and calendar invitations; use the host's display name and Reply-To, plus an optional guest footer. [Email notifications](./booking-flow.md#email-notifications), [SMTP](./admin.md#smtp-configuration) |
| SMS | Send guest confirmations, cancellations, reschedules and reminders through Twilio, GatewayAPI, seven.io or a webhook gateway, with a daily segment limit. [SMS](./sms.md) |
| Meeting locations | Use static video links, phone, in-person or custom locations, generated Jitsi rooms, Google Meet or a meeting-provider webhook. [Locations](./event-types.md#location) |
| Localization and appearance | Use ten languages plus formal German, light/dark modes, preset/custom themes, company logos for each mode and a favicon. [Profile settings](./dashboard.md#profile-and-settings), [branding](./admin.md#branding) |

## Administration and integrations

| Capability | What you can do |
|---|---|
| Identity | Use local accounts with optional/required TOTP and recovery codes, or OIDC SSO with group sync. [Authentication](./authentication.md), [Authentik](./authentik.md) |
| Administration | Manage users, groups and instance policies, impersonate users for troubleshooting, inspect booking and calendar health. [Admin dashboard](./admin.md) |
| Webhooks | Deliver signed booking lifecycle events with retries and stable delivery IDs. [Webhooks](./webhooks.md) |
| REST API and OpenAPI | Use scoped OAuth or personal access tokens for event types, slots and booking operations, including optional question answers; use `/api/docs` for the interactive reference. [REST API](./api.md) |
| MCP | Authorize an OAuth-capable Streamable HTTP client to list event types, create invites and read cached busy intervals. [MCP](./mcp.md) |
| Operator CLI | Manage sources, sync, event types, bookings, users, time off and configuration; import calendar OAuth credentials and export non-secret configuration. [CLI reference](./cli-reference.md) |
| Self-hosting | Run a container or systemd service, keep SQLite and credential-key backups, configure trusted proxies and private calendar hosts. [Deployment](./deployment.md), [security](./security.md) |

These integrations have different scopes. MCP does not create bookings; use the REST API for supported booking operations. Calendar OAuth connects calendars; OIDC configures sign-in. A meeting-provider webhook returns a video URL; an outgoing booking webhook reports a lifecycle event.
