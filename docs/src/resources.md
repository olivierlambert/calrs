# Shared Resources

Shared resources are instance-level bookable assets: a demo lab, a meeting room, a shared piece of equipment. Each resource is backed by a read-only ICS publish feed (a BlueMind "calendar address", a Nextcloud public link, or any URL serving an iCalendar file). Once a resource is attached to an event type, a busy resource blocks booking slots the same way a busy calendar does.

The feature is opt-in. With no resources configured, nothing changes anywhere in the booking flow.

## How it fits together

1. A global admin adds a resource in the admin panel, pointing at its ICS feed.
2. The resource is attached to one or more event types ("Required resources" on the event type form).
3. When guests view the slot picker, times where the resource is busy are hidden.
4. When a booking is confirmed, calrs can write a reservation event into the resource's own CalDAV calendar, so other tools (and other calrs event types) see it as busy too.
5. When the booking is cancelled or rescheduled, the reservation is released.

Even without CalDAV write-back, two event types sharing a resource cannot double-book it: calrs merges the feed events with its own confirmed bookings when computing busy times.

## Adding a resource

Go to **Dashboard > Admin > Resources** and click **Add resource**:

- **Feed URL** (required) is the read-only ICS publish URL. It is validated and synced immediately on create, and the resource name is auto-filled from the feed's `X-WR-CALNAME` if present.
- **CalDAV URL** (optional) is the writable CalDAV collection for reservation write-back. For BlueMind publish URLs, calrs can derive it automatically, so you can usually leave it blank.
- **Service account** (optional) is a username and password with write access to the CalDAV collection. The password is encrypted at rest (AES-256-GCM, same as other stored credentials). When editing, leaving the password field empty keeps the current value.

Per-resource actions:

- **Sync now** forces an immediate feed re-sync.
- **Test write** verifies write-back end to end: it PUTs a temporary event 24 hours out, checks it exists, then deletes it.
- **Edit** and **Delete** work as expected. Deleting a resource detaches it from all event types.

### Feed sync and failures

Feeds are cached and re-synced automatically when older than 5 minutes, on demand as slots are computed. The feed is authoritative: events that disappear from the feed are removed from the cache.

If a feed fetch fails, the admin panel shows a sync failure indicator with the last error. Failed fetches still update the sync timestamp so dead feeds back off instead of being retried on every request. A successful sync clears the error.

## Attaching resources to event types

The event type form gains a **Required resources** section listing the resources you may attach, plus a scheduling mode selector.

- **Personal event types:** only global admins see and edit the section.
- **Team event types:** global admins always can. Team admins can too, if their team is on the resource's allowlist (see [Team allowlist](#team-allowlist) below).

## Scheduling modes

When an event type has more than one required resource, the **Resource scheduling** mode controls how they combine:

| Mode | Slot is available when | At booking time |
|---|---|---|
| **All** (default) | Every attached resource is free | The booking blocks all attached resources |
| **Round-robin** | At least one attached resource is free | The least-loaded free resource is picked and assigned to the booking |

Use **all** when a meeting genuinely needs every resource (a room and a projector). Use **round-robin** when any one of several interchangeable resources will do (three identical demo labs). In round-robin mode, the assigned resource is stored on the booking and shown on the host's bookings dashboard and in host-facing emails. Guests never see resource names.

## How busy resources block slots

For each candidate slot, calrs merges, per resource:

- Feed events, including expanded recurring events, skipping cancelled and transparent (free) ones
- calrs' own confirmed bookings that hold that resource

In **all** mode, the union of busy intervals blocks the slot. In **round-robin** mode, only the intersection blocks it (the slot survives as long as one resource is free). Pending bookings do not block resources; both approval paths re-check resource availability before confirming, so an approval fails cleanly if the resource was taken in the meantime.

The **Troubleshoot** view shows resource conflicts as `resource_busy` intervals, alongside calendar events and bookings.

## Reservation write-back

When a booking is confirmed, calrs PUTs a reservation event into the resource's CalDAV collection under the booking's own UID:

- In **all** mode, into every attached resource.
- In **round-robin** mode, only into the assigned resource.

When a confirmed booking is cancelled or rescheduled, the reservation is deleted. The targets are derived from the stored assignment, so a resource that was detached from the event type after booking still gets released. Declined pending bookings need no cleanup, since reservations are only pushed on confirmation.

Write failure is never fatal: the booking still succeeds, the DB-side busy check keeps blocking the resource, and the failure is logged.

### Credentials for write-back

calrs tries credentials in trust order:

1. The resource's **service account**, if configured. This is always preferred.
2. Members who opted in to **credential lending** and have a CalDAV source on the same scheme, host, and port as the resource's CalDAV URL. The booking's assigned host is preferred among them.

Members opt in from **Profile & Settings**. Note that lending grants writes to any collection on that origin that the member's own server-side ACL allows, so a dedicated service account is the safer setup.

## Team allowlist

By default, only global admins can attach resources to event types. To delegate this to team admins, configure the allowlist on each resource:

1. In **Dashboard > Admin > Resources**, edit a resource and set **Teams allowed to use this resource**.
2. Team admins of an allowlisted team then see the **Required resources** section on their team event type forms, restricted to the resources allowlisted for their team, and can attach or detach them.

Attachment is validated server-side against the allowlist, not just hidden in the UI. An empty allowlist keeps the old behavior (global admins only). Personal event types remain global-admin only regardless of allowlists.

The allowlist controls **attachment only**. Availability evaluation is unchanged: once a resource is attached, it blocks slots for everyone booking that event type.

## CLI

Probe a resource URL before adding it, to check what calrs can do with it:

```bash
# Probe an ICS publish feed or CalDAV collection
calrs resource probe --url https://mail.example.com/api/calendars/publish/...

# Authenticated CalDAV probe (password prompted)
calrs resource probe --url https://mail.example.com/dav/... --username svc-resources

# Also run a write test (PUT a temporary event, verify, delete)
calrs resource probe --url https://... --username svc-resources --write-test
```

`calrs event-type slots` consults attached resources, so the printed slots match the web booking page. `calrs booking cancel` releases the booking's resource reservation on cancellation. See the [CLI Reference](./cli-reference.md) for details.
