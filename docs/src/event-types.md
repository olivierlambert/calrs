# Event Types

Event types are bookable meeting templates. Each one defines the duration, availability schedule, and booking rules.

## Creating an event type

### From the dashboard

Go to **Dashboard > Event types > + New** and fill in:

- **Title** — display name (e.g., "30-minute intro call")
- **Slug** — URL path (e.g., `intro` gives `/u/yourname/intro`)
- **Duration** — meeting length in minutes
- **Buffer before/after** — padding between meetings (prevents back-to-back bookings)
- **Minimum notice** — how far in advance guests must book (in minutes)
- **Requires confirmation** — if checked, bookings start as "pending" and you approve from the dashboard
- **Location** — video link, phone number, in-person address, or custom text
- **Availability schedule** — which days and hours you're available

![Event type edit form](images/event-type-form.png)

### From the CLI

```bash
calrs event-type create \
  --title "30min intro call" \
  --slug intro \
  --duration 30 \
  --buffer-before 5 \
  --buffer-after 5
```

## Calendar selection

When you have multiple CalDAV calendars, you can choose **which calendars block availability** for each event type. For example, a "Work meeting" event type can check only the work calendar, while a "Personal chat" checks only the personal calendar.

From the dashboard form, select the calendars under the **Calendars** section. Only calendars marked as "busy" (`is_busy=1`) appear.

**Default behavior:** If no calendars are selected, all busy calendars are checked — same as before. This is fully backward-compatible.

## Availability schedule

Each event type has its own availability rules. By default: Monday–Friday, 09:00–17:00.

From the dashboard form, you can set:

- Which days of the week are available (checkboxes)
- Start and end time for available hours

The availability engine intersects these rules with your synced calendar events (filtered by selected calendars) and existing bookings to compute free slots.

## Slot computation

Available slots are computed by:

1. Generating candidate slots from availability rules (day of week + time range)
2. Filtering out slots that overlap with calendar events (from CalDAV sync)
3. Filtering out slots that overlap with confirmed bookings
4. Applying buffer times (before and after each slot)
5. Removing slots that violate minimum notice (too close to now)

```bash
# View available slots for the next 7 days
calrs event-type slots intro

# View slots for the next 14 days
calrs event-type slots intro --days 14
```

## Location

Event types support four location types:

| Type | Description |
|---|---|
| `link` | Video meeting URL (Zoom, Meet, etc.) |
| `phone` | Phone number |
| `in_person` | Physical address |
| `custom` | Free-text description |

The location is displayed on the public booking page, in confirmation emails, and in `.ics` calendar invites.

## Enabling/disabling

Event types can be toggled on/off from the dashboard without deleting them. Disabled event types don't show up on your public profile and return 404 on their booking page.

## Private event types

Event types can be marked as **private** to hide them from your public profile page and group pages. Private event types are only accessible via invite links — guests cannot find or book them directly.

### Enabling private mode

In the event type form, check the **Private** checkbox under the Notifications section. Private event types show an indigo "private" badge on the dashboard.

### Invite links

Private event types use **booking invites** to grant access:

1. Go to **Dashboard > Event Types** and click **Invite** on a private event type
2. Fill in the guest's name, email, and an optional personal message
3. Choose an expiration (7, 14, or 30 days, or never) and whether to allow multiple bookings
4. Click **Send invite** — the guest receives an email with a personalized booking link

The invite link takes the guest directly to the slot picker with the invite token embedded. Their name and email are pre-filled on the booking form. The token is validated at every step (expired, used-up, or invalid tokens are rejected).

### Use case: sales-qualified demos

A common pattern is to create a **private group event type** for a demo team:

1. Create a group event type (e.g., "Product Demo") under the Demo group
2. Mark it as private
3. Sales reps (who have dashboard access) can send invites to qualified leads
4. The demo is automatically assigned to the least-busy team member via round-robin
5. The sales rep never needs to coordinate calendars manually

### Invite management

The invite management page (`/dashboard/invites/{event_type_id}`) shows:

- A form to send new invites
- A list of sent invites with status badges:
  - **Active** — invite is valid and unused (or has remaining uses)
  - **Expired** — past the expiration date
  - **Used** — all uses consumed (for single-use invites)
- Delete button to revoke an invite

## Public URLs

![Public profile page](images/profile.png)

- **Profile:** `/u/yourname` — lists all enabled, non-private event types
- **Slot picker:** `/u/yourname/slug` — shows available time slots
- **Booking form:** `/u/yourname/slug/book?date=...&time=...` — booking form for a specific slot
- **Invite booking:** same URLs with `?invite={token}` — for private event types accessed via invite links
