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

## Availability schedule

Each event type has its own availability rules. By default: Monday–Friday, 09:00–17:00.

From the dashboard form, you can set:

- Which days of the week are available (checkboxes)
- Start and end time for available hours

The availability engine intersects these rules with your synced calendar events and existing bookings to compute free slots.

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

## Public URLs

![Public profile page](images/profile.png)

- **Profile:** `/u/yourname` — lists all enabled event types
- **Slot picker:** `/u/yourname/slug` — shows available time slots
- **Booking form:** `/u/yourname/slug/book?date=...&time=...` — booking form for a specific slot
