# Microsoft Calendar

calrs connects to Microsoft 365 (Exchange Online) and Outlook.com calendars through **native Microsoft Graph**, using delegated OAuth2 consent. The operator registers an application once; each user then connects their own Microsoft account and chooses a calendar for booking write-back.

This is separate from **Microsoft Exchange on-premises (EWS)**, which uses the Exchange source configuration rather than Graph. It is also separate from logging into calrs: connecting a calendar neither enables Microsoft SSO nor changes your calrs login identity. Your Microsoft mailbox address does not have to match your calrs account email.

## 1. Register an application in Microsoft Entra

Open the [Microsoft Entra admin center](https://entra.microsoft.com/), then **App registrations → New registration**. Give the application a recognizable name, such as `calrs`.

Choose **Supported account types** for the users you intend to serve, and use the corresponding tenant value in calrs:

| Intended audience | App registration audience | calrs tenant value |
|---|---|---|
| One organization | Accounts in this organizational directory only | That directory's tenant UUID or verified domain, such as `example.onmicrosoft.com` |
| Multiple organizations | Accounts in any organizational directory | `organizations` |
| Organizations and personal Outlook.com accounts | Accounts in any organizational directory and personal Microsoft accounts | `common` |
| Personal Microsoft accounts only | Personal Microsoft accounts only | `consumers` |

The tenant value and app audience must agree. `common` does not make a single-tenant application multi-tenant. calrs accepts `common`, `organizations`, `consumers`, a tenant UUID, or a domain-shaped tenant value; use a domain verified for your intended tenant, not an arbitrary host or a login URL. The account must also have a calendar mailbox accessible through Graph; having an Entra identity alone is not sufficient.

The integration uses the global-cloud endpoints `login.microsoftonline.com` and `graph.microsoft.com`. Sovereign/national clouds and custom Graph endpoints are not supported.

## 2. Register the exact Web callback

Add a **Web** platform redirect URI (not a single-page application or mobile/desktop redirect):

```text
https://cal.example.com/dashboard/sources/microsoft/callback
```

Replace `https://cal.example.com` with your instance's public base URL. The callback path is exactly `/dashboard/sources/microsoft/callback`, with no trailing slash.

The public base URL configured for calrs must be an **HTTPS origin**: scheme and host, with an optional port, but no subpath, credentials, query, or fragment. HTTP is rejected by this integration, including for local development. If TLS terminates at a reverse proxy, configure calrs with the external HTTPS origin, not its internal HTTP listener address.

The Entra redirect URI must match the callback generated from that origin. Register separate redirect URIs or applications for separate environments. No implicit-grant setting is needed: calrs uses the authorization-code flow with PKCE and a client secret.

## 3. Configure delegated permissions

Under **API permissions**, add these Microsoft Graph **delegated** permissions:

| Permission | Purpose |
|---|---|
| `Calendars.ReadWrite` | Discover/read calendars and create, update, and delete booking appointments on the user's behalf. |
| `User.Read` | Read `/me` to identify the connected Microsoft account. |
| `offline_access` | Obtain a refresh token so synchronization can continue when the user is not actively connecting. |

calrs requests `https://graph.microsoft.com/Calendars.ReadWrite`, `https://graph.microsoft.com/User.Read`, and `offline_access`. It requires both calendar and profile consent and an initial refresh token. Do **not** substitute application permissions: this is not app-only mailbox access or a client-credentials integration.

Users consent when they connect. An organization's consent policy may require an administrator to approve the application even though the flow uses delegated permissions. Arrange that approval with the tenant administrator; logging into calrs, including through an OIDC provider, does not grant Microsoft calendar permissions.

## 4. Create a secret and configure calrs

In Entra, open **Certificates & secrets → Client secrets → New client secret**. Choose an expiry suitable for your rotation policy and copy the secret **Value** immediately. The **Secret ID** is not a credential and will not work.

In the calrs **Admin** dashboard (`/dashboard/admin`), find the **Microsoft Calendar** card and save:

- **Client ID:** the app registration's **Application (client) ID**, not its object ID.
- **Client secret:** the secret **Value**.
- **Tenant:** the value selected in step 1.

The card displays the callback URL to register, or, while the public base URL is not an HTTPS origin, what to change first. The secret is stored encrypted and is not displayed again; leaving its input blank preserves the current secret.

Track the secret's expiry outside calrs. Before it expires, create a replacement secret for the same application, save its value in calrs, and verify calendar access and token refresh before retiring the old secret. An expired or revoked secret prevents new authorizations and token refreshes; an already-issued access token can temporarily conceal the problem. If authorization has been revoked or refreshing still fails after correcting the credentials, reconnect the affected source. Restart any in-progress connection after changing the client ID or tenant.

## 5. Connect a user's calendar

1. Log into calrs as yourself. Microsoft connection and reconnection are not available while an administrator is impersonating a user.
2. Open **Sources**, add a source, and use the Microsoft Calendar connect action.
3. Select the intended Microsoft account and grant the requested permissions. Keep the same calrs login session throughout the flow.
4. calrs discovers the calendars and starts the initial sync in the background, so a large mailbox does not hold up the redirect. On a new connection without a selected write-back calendar, it opens the write-back selection page; busy times appear once the sync finishes. Reconnecting a disabled source renews its credentials but leaves it disabled and unsynced.
5. Select a calendar you can write to and save. You can skip write-back if you only need busy-time checks, or change the destination later using **Write bookings to** on the source. Discovery alone does not prove write permission; verify a booking as described below.
6. In each event type's conflict-calendar settings, select the Microsoft calendars that should contribute busy time. This selection is separate from the source's write-back destination.

Microsoft calendars are discovered through `/me/calendars`; the integration is not an organization-wide mailbox browser. The write-back destination receives booking appointments, while conflict-calendar selection controls which calendar events affect availability.

Use **Reconnect** on the existing source if consent or refresh access is lost. Choose the **same Microsoft identity** that originally connected it: calrs checks the stable Microsoft user ID rather than matching email strings. A successful reconnect updates credentials while preserving the source and its write-back selection. Selecting a different identity is rejected; add a new source to connect a different account instead.

### Calendars shared by colleagues

calrs treats a listed calendar whose owner is not the connected account as someone else's, and a calendar Graph reports as not editable as read-only. Graph reports both for every calendar it lists, whatever kind of share is behind it:

- A calendar owned by someone else does **not** make the user busy unless an event type selects it in its conflict-calendar settings. An event type with no selection checks only the user's own calendars, so a colleague's meetings do not block the user's booking page by default.
- A read-only calendar is **not** offered as a write-back destination, since Graph refuses writes to it. This includes the user's own read-only calendars, such as holidays and birthdays.

Which shared calendars Graph lists, and whether it lets calrs read them, is up to Exchange and is not documented by Microsoft. On Exchange Online, in our testing, a colleague's calendar shared and accepted in Outlook was listed among the user's own calendars and readable with `Calendars.ReadWrite`, while one added with free/busy access only was not listed at all. If Graph refuses to read a calendar it lists (403 or 404), that calendar is skipped with a warning in the log rather than failing the source, unless it is the write-back calendar or no calendar of the source can be read.

When a calendar is no longer listed, because it was deleted or its share was removed, the next sync drops it and its events. An event type that had selected only that calendar checks the user's own busy calendars again.

## Synchronization and booking behaviour

- **Window:** synchronization reads a full snapshot from 90 days before the sync through 730 days after the sync. Events outside that window are not covered by the current snapshot; do not rely on Microsoft conflict checks beyond the forward horizon.
- **Recurrence:** Graph's `calendarView` expands recurring events server-side, including exceptions. calrs explicitly requests `originalStart`, which Graph omits by default, to preserve each instance's original identity when it moves. Full-snapshot reconciliation removes cached occurrences that Graph no longer returns, including cancelled occurrences. This provider does not use delta cursors.
- **Timed events:** Graph is asked for UTC times, which calrs converts for scheduling in the host's timezone.
- **All-day events:** calendar dates are preserved, with an exclusive end date. A busy event starting on March 29 and ending on March 31 blocks March 29 and March 30 as whole host-local days, including across daylight-saving transitions; it is not treated as a fixed 48-hour UTC interval.
- **Busy status:** `free` and `tentative` do not block availability. Busy, out-of-office and `workingElsewhere` events do, as on Exchange on-premises (EWS); cancelled events do not. A meeting invitation the user has not answered shows as tentative in Outlook, so it does not block either. A private event on a colleague's calendar keeps its time and status, under the subject "Private Appointment".
- **Unreadable events:** an event calrs cannot interpret is skipped, and the rest of the calendar still syncs. Until a sync reads every event, no cached event is removed from that calendar. The source's sync is reported as failing meanwhile, naming the calendar, and the owner is alerted as for any other sync failure.
- **Throttling:** a request Graph throttles (429) or cannot serve (503, 504) is retried up to three times, honouring `Retry-After` up to 30 seconds. A sync that still fails is retried later and keeps the cached busy times meanwhile.
- **Write-back:** confirmed bookings create appointments in the selected calendar. Rescheduling updates the existing Graph item, and cancellation deletes it. calrs tracks its booking identity in a private extended property, separately from Graph's read-only `iCalUId`; event bodies are neither read during sync nor kept in the cache.
- **Invitations:** calrs sends its own booking invitations. Graph write-back does not add attendees or an organizer and does not generate a second set of Outlook meeting invitations. Connecting Graph does not by itself generate a Teams meeting link.

## Verify your connection

These are manual checks to perform against your own account. Use a disposable appointment and booking, and avoid notifying unintended guests.

1. **Discovery:** after connecting, confirm that the expected calendar names appear and select a writable destination. Run the source's sync action and check its last-sync status and any error message.
2. **Busy blocking:** create a short appointment marked **Busy** in a selected conflict calendar during otherwise available hours within the next week. Sync the source, then open your public booking page and confirm that the overlapping slot is unavailable. The event type's **Troubleshoot** view helps distinguish calendar conflicts from buffers or availability rules.
3. **Recurrence and all-day:** optionally repeat with a recurring appointment, move one occurrence, and cancel another. After syncing, check the resulting slots. Test a busy all-day event in your host timezone and confirm that it blocks the intended date, not an adjacent day.
4. **Create:** make a test booking using an address you control. If the event type requires approval, confirm it. Check that exactly one appointment appears in the selected Microsoft write-back calendar with the expected time and details.
5. **Reschedule:** reschedule through calrs and confirm that the same Outlook appointment moves, rather than leaving an old copy or creating a duplicate.
6. **Cancel:** cancel through calrs and confirm that the appointment disappears from Outlook. Remove any manually created test appointments when finished.

A successful Microsoft consent screen or source connection test is not a substitute for these busy-time and write-back checks.

## Troubleshooting

| Symptom | What to check |
|---|---|
| Microsoft connect is unavailable or cannot start | Save a client ID and secret in the admin card, check the tenant value and public HTTPS origin, and leave impersonation mode. |
| Redirect mismatch (for example `AADSTS50011`) | Compare Entra's **Web** redirect URI with the exact callback shown in calrs. Check scheme, host, port, path, and trailing slash; do not use the proxy's internal address. |
| Account is not permitted to use the application | Match the app registration audience to `common`, `organizations`, `consumers`, or the intended tenant. A personal account cannot use an organizational-only application. |
| “Need admin approval,” consent denied, or access blocked by policy | Ask the Microsoft tenant administrator to review delegated permissions, user-consent restrictions, application assignment, and Conditional Access policy. Do not attempt to bypass tenant policy by granting application permissions. |
| Authorization rejected or refresh stops working | "Reconnect the source" means Microsoft refused the refresh token (expired, revoked, or a sign-in the tenant now requires): calrs stops refreshing it, so use **Reconnect**. "An administrator must check the client ID, secret and tenant" points at the app registration: check the client ID, secret **Value** (not ID), secret expiry and tenant. |
| Write-back fails with "delete the copy" | The booking's appointment was copied in Outlook, and the copy carries the same calrs identity. Delete the copy; the next write-back retry succeeds. |
| Invalid or expired authorization state | Start again from Sources while logged into the same calrs account. The callback is single-use and expires after ten minutes; do not reuse a callback URL from browser history. |
| Consent succeeds but initial calendar sync fails | Confirm that the Microsoft identity has an accessible Exchange Online or Outlook.com mailbox and calendar consent. Inspect the source's sync error, then retry sync after fixing access. The source may already have been saved; check Sources before adding another. |
| Reconnect fails after account selection | Select the original Microsoft identity. Use a new source for a different account. |
| Busy event does not remove a slot | Check the event type's conflict-calendar selection, the sync status, the 730-day forward horizon, and Outlook's **Show as** value. Free and tentative events are treated as free. |
| A colleague's shared calendar does not affect availability | Calendars shared by others are not checked by default. Select it in the event type's conflict-calendar settings. |
| A calendar is missing from **Write bookings to** | Read-only calendars (shared by colleagues, holidays, birthdays) are not offered. Ask the owner for edit access, or write to one of your own calendars. |
| Booking exists in calrs but not in Outlook | Check the source's **Write bookings to** selection and actual write access to that calendar, as well as current consent and credentials. Read-only discovery does not establish write access. |

Do not include client secrets, tokens, or OAuth callback query strings in support reports. Share the operation that failed, a sanitized error, and whether discovery, busy-time sync, or booking write-back was affected.
