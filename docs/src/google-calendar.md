# Google Calendar

calrs can connect to Google Calendar as a CalDAV source using Google's OAuth2 flow. Unlike a username/password source, Google requires you (the calrs operator) to register an OAuth2 application in Google Cloud and configure its client ID and secret in calrs once. After that, individual users connect their own Google accounts through the dashboard.

This page walks through the Google Cloud setup. The values you collect at the end go into **Admin → Sign-in and security → Google OAuth2** in the calrs dashboard.

---

## 1. Pick (or create) a Google Cloud project

Go to the [Google Cloud Console](https://console.cloud.google.com/) and either select an existing project or create a new one (e.g. `calrs-prod`). Everything below is scoped to this project.

The project is just a container for the OAuth2 client and the enabled APIs. You can use the same project for staging and production, or split them. It makes no functional difference to calrs.

---

## 2. Enable the required APIs

In **APIs & Services → Library**, enable:

- **CalDAV API**: used for CalDAV access to the user's calendars (reading busy times and writing bookings back). It is a separate entry in the Library from the Calendar API; search for "CalDAV API".
- **Google Calendar API**: used to add the guests to written events without Google mailing them (see [Invitations](#invitations-and-booking-email-aliases)), to read out-of-office events, and to attach Google Meet links to bookings (see [Google Meet auto-links](#google-meet-auto-links)).
- **OIDC userinfo**: Google exposes this automatically when you request the `openid email` scopes; there is no separate "API" to toggle, but the OAuth consent screen must allow those scopes (see step 5).

Without the CalDAV API enabled, every CalDAV request will fail with a 403 even though the OAuth2 handshake itself succeeds; enabling only the Google Calendar API is not enough. Without the Google Calendar API, bookings still sync and write back, but without the guests as attendees, out-of-office events do not block, and Google Meet links cannot be generated.

---

## 3. Create the OAuth2 client

In **APIs & Services → Credentials**, click **Create credentials → OAuth client ID**.

- **Application type:** *Web application*
- **Name:** anything (e.g. `calrs`)

You will get a **client ID** and **client secret**. calrs stores the client ID and encrypts the client secret at rest once you save them in the admin panel.

---

## 4. Authorized redirect URI

Under **Authorized redirect URIs**, add **exactly** one entry:

```
{CALRS_BASE_URL}/dashboard/sources/google/callback
```

Replace `{CALRS_BASE_URL}` with the public URL of your calrs instance, e.g.:

```
https://cal.example.com/dashboard/sources/google/callback
```

Notes:

- The URI must match byte-for-byte. No trailing slash, correct scheme (`https://` in production), correct host. Google rejects the callback otherwise.
- `CALRS_BASE_URL` is the same env var calrs uses for OIDC redirects and email links. Keep them consistent.
- If you run multiple environments (staging + prod), either register the same client with multiple redirect URIs or create one OAuth2 client per environment.

You do **not** need to set "Authorized JavaScript origins"; calrs performs the redirect server-side.

---

## 5. Scopes calrs requests

When a user connects their Google account, calrs requests these scopes:

| Scope | Why |
|---|---|
| `https://www.googleapis.com/auth/calendar` | Full read/write access to the user's calendars via Google's CalDAV endpoint. Needed both to read busy times and to push confirmed bookings back to the calendar. |
| `openid email` | OpenID Connect userinfo, used once at connect time to discover the account's email address. Google's CalDAV principal URL is `/caldav/v2/{userEmail}/user`, so calrs needs to know which email to scope the source to. |

calrs also passes `access_type=offline` and `prompt=consent` so that Google issues a long-lived refresh token. The refresh token is stored encrypted and used to mint new access tokens as needed (existing tokens are rotated automatically).

On the OAuth consent screen configuration (**APIs & Services → OAuth consent screen**), add the Calendar scope explicitly. The `openid` and `email` scopes are listed under the "non-sensitive" defaults and don't need additional review.

The same Calendar scope is enough to attach a Google Meet conference to a write-back event. No extra Meet scope, and users who already connected Google Calendar do not need to reconnect.

---

## 6. Test users vs. publishing the consent screen

For an **External** application in **Testing**, add the intended accounts as test users. Because calrs requests Calendar access, refresh tokens issued in that mode expire after seven days; users then need to reconnect. See [Google's refresh-token expiration rules](https://developers.google.com/identity/protocols/oauth2#expiration).

For ongoing use, configure the application's production audience and review Google's verification requirements. Publishing and verification are separate steps: eligible personal-use and organization-internal apps can qualify for exceptions. An unverified external app can still show a warning and have a user cap; a Workspace administrator can also restrict consent. See [Google's verification exceptions](https://support.google.com/cloud/answer/13464323) and [sensitive-scope verification](https://developers.google.com/identity/protocols/oauth2/production-readiness/sensitive-scope-verification).

After moving out of Testing, reconnect affected sources to obtain fresh authorization. Production tokens can still be revoked or expire for other reasons; check source errors rather than assuming a token lasts forever.

## Out-of-office blocking

calrs also reads Google Calendar's **Out of office** events through the Calendar
API using the existing OAuth connection. No additional scope or reconnect is
required. These events block availability on their connected calendar, respecting
the calendar's busy toggle and per-event-type calendar selection.

Google represents these as timed, busy intervals, including absences that cover
whole days. Recurring absences are expanded by Google, including moved occurrences
and daylight-saving changes. Cancelled occurrences are omitted from the returned
snapshot and stop blocking after the next successful sync. Each sync refreshes a window
from 90 days ago to 366 days ahead, covering the web booking flow's 365-day limit.
Deleted absences are removed on the next successful sync. If the API request
fails, previously cached absences keep blocking, and the source's sync is
reported as failing on the Sources page (with the usual email to its owner
when it keeps failing). After 7 days without a successful refresh the cached
absences are dropped, so an absence deleted meanwhile does not block forever.
A Calendar API that is not enabled at all is not reported, as long as nothing
was cached from it. The absences are fetched at most every 5 minutes per
calendar, and a fetch that takes over 10 seconds counts as a failure rather
than holding up a booking page. An absence Google marks as a whole day (a `date` rather
than a time) blocks those days in the calendar's own timezone. Working-location
events are not treated as absences.

## Invitations and booking email aliases

calrs sends the guest and any additional attendees its own confirmation email with an ICS invitation. Google Calendar must not send a second one, yet the guests belong on the event: Google Meet admits listed attendees directly instead of putting them in the "ask to join" lobby.

Google's CalDAV endpoint ignores `SCHEDULE-AGENT=CLIENT` (RFC 6638), the parameter other servers honour to leave invitations to the client, and mails attendees itself. So on a Google calendar calrs writes in two steps:

1. The event goes over CalDAV without ATTENDEE or ORGANIZER lines, so there is nobody for Google to invite. Google makes the calendar's own account the organizer, which is also what lets that account create the Meet.
2. The guest and additional attendees are then added through the Google Calendar API with `sendUpdates=none`, which Google documents as "No notifications are sent."

Rewriting an existing event (a reschedule, a retry) first clears its attendees the same silent way, so the CalDAV write never reads as removing them, then adds them back; their RSVP on the Google event is reset in the process, without any notice. If adding them back fails, the event stays on the calendar and the write-back retries until they are there. Google Meet time changes and the conference itself are patched with `sendUpdates=none` too, and a cancelled booking whose Google event has attendees is deleted through the Calendar API with `sendUpdates=none`. If that call fails, calrs deletes the event over CalDAV instead, so the cancelled meeting never stays on the calendar; Google may then mail the attendees its own cancellation. A copy left behind when a booking moves to another calendar (a write calendar chosen per event type) is only ever deleted that quiet way: if the Calendar API cannot delete it, it stays, still listing the guests beside the new copy, and calrs tries again on the write-back schedule. A calendar several hosts write to (a shared team calendar) gets one copy of the booking, the one with the guests. A chosen Google calendar that was deleted counts as refusing the booking, so it goes to the source's write calendar instead. Guests with a Google account may still see the event appear in their Google Calendar, which is how Google handles any event they are an attendee of; they get no email from Google.

Only one Google copy of a booking carries the attendees. Google adds an event to the Google Calendar of each of its attendees, so a booking written to several Google calendars (every member of a collective event type, a second Google calendar of the host, the calendar of a watcher who claimed the booking) would otherwise show there once per copy. The copy with the attendees is the one holding the Google Meet when the booking has one, otherwise the host's: the assigned member for round-robin, the organizer for collective (the member the invitation names as organizer), the owner for a personal event type, on the Google calendar a Meet would be created on. That copy leaves out the guest's phone number, since Google shows the event's description to everyone on it; the other copies keep it for their host. Every other Google copy is written without attendees, and one that has some from an earlier write loses them, silently, the next time it is written. When that host does not write to a Google calendar, no Google copy carries attendees. For a collective booking the organizer is fixed when it is booked: an organizer who later leaves the team keeps the copy with the attendees until the booking is over. A watcher who claimed the booking is left off that list when calrs writes a copy to their own calendar, so they do not see the meeting twice; without such a copy they stay on it and get the meeting through the invitation.

This needs the **Google Calendar API** enabled (step 2 of the setup). Without it, calrs still writes a new event, without attendees, and the guests rely on calrs's own invitation; cancellations then go over CalDAV. An event that already has attendees (the API was turned off after it was written) is left as it is rather than rewritten without them, which Google would announce. When the Calendar API is enabled but fails for a moment (a rate limit, a server error, no answer within 30 seconds), calrs writes nothing and the [write-back retries](caldav.md#when-a-write-back-fails) pick the booking up again a few minutes later. Other CalDAV servers are unaffected: they keep receiving the attendees in the ICS with `SCHEDULE-AGENT=CLIENT`.

The guest-facing ICS uses your **Booking email** (or account email if unset) as the organizer, which can differ from your connected Google address. The email's From address is controlled separately by your SMTP configuration.

## Google Meet auto-links

When an event type's location is **Google Meet (auto-generated link)**, confirmed bookings attempt to attach a unique Meet conference owned by the host, as long as that host still has Google Calendar connected with write-back:

1. calrs writes the booking to the host's Google write-back calendar (same CalDAV PUT as any other booking).
2. It then calls Google Calendar API `events.patch` with `conferenceData.createRequest` (`hangoutsMeet`) using the host's existing OAuth token.
3. The Meet URL is stored on `bookings.meeting_url` and included in confirmation emails, the guest ICS, and CalDAV copies on any other calendars.

No extra OAuth scope or admin field is required beyond the setup above, as long as the Google Calendar API was enabled in step 2. Users who already connected Google Calendar do not reconnect.

### Who owns the Meet

| Booking | Meet owner |
|---|---|
| Personal event type | The event type owner |
| Team round-robin | The assigned member |
| Team collective | The first eligible member by name when the booking was made (same person as the calendar ORGANIZER, kept even if the team changes later) |
| Dynamic group (`/u/alice+bob/...`) | The event type owner (first username). Checked on the slots page so the guest is not asked to fill the form first. Other participants do not need Google connected. |

The conference is created with that person's token on their write-back calendar, so they see the native "Join with Google Meet" button in Google Calendar.

### Save-time gate

You cannot save an event type as Google Meet unless every scheduling-relevant host already has Google Calendar connected **and** has picked a write-back calendar:

- Personal: you
- Team: every eligible member (enabled, non-zero weight)

The form lists anyone still missing. If someone later disconnects Google, the booking still confirms; confirmation and email show a “Google Meet” label if minting failed (the Meet URL is omitted and a warning is logged, same as a failed meeting webhook).

### Reschedule and cancel

Reschedule keeps the same Meet URL and patches only the event times via Calendar API (a second CalDAV PUT would strip the conference). Cancel deletes the Google event and its Meet, through the Calendar API with `sendUpdates=none` since the event lists the guests.

That time patch is the host's only copy of the booking, so it is retried three times a second apart. If it still fails, the host is emailed: the booking, the guest's invite and the reminders hold the new time while their Google Calendar holds the old one, and they need to move the event by hand. Move it rather than recreating it, or the Meet link the guest already has stops admitting anyone.
