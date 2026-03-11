# Team Links

Team links let you create ad-hoc, shareable booking links across any combination of calrs users — without needing an admin to create a formal group.

## How team links work

- Any user can create a team link from the dashboard
- Pick team members from all enabled calrs users (checkboxes)
- A slot is available only when **all** selected members are free
- The guest books via a public link (`/t/{token}`)
- The booking is pushed to every member's CalDAV calendar
- The link is **one-time use** — it auto-deletes after a successful booking

## Team links vs groups

| | Team links | Groups |
|---|---|---|
| **Who creates them** | Any user | Admin (via OIDC/Keycloak) |
| **Members** | Hand-picked from all users | Synced from OIDC groups claim |
| **Availability logic** | ALL members must be free | ANY member free (round-robin) |
| **Assignment** | Everyone is booked | One member assigned |
| **Lifetime** | One-time use, auto-deleted | Permanent |
| **Use case** | Ad-hoc meetings with specific people | Recurring team booking pages |

## Creating a team link

From the dashboard:

1. Click **+ New** under "Team links"
2. Enter a title (e.g. "Product demo with sales team")
3. Set duration, buffer times, and minimum notice
4. Configure the availability window (days and hours)
5. Select team members (you are always included)
6. Click **Create team link**

The link appears in the "Team links" section of your dashboard. Use the **Copy link** button to share it.

## Public booking flow

1. Guest visits `/t/{token}`
2. Sees available slots (times where all team members are free)
3. Picks a slot and fills in their details
4. Booking is confirmed:
   - Event pushed to every member's CalDAV calendar
   - Email notification sent to all members
   - Confirmation email sent to the guest
5. The team link is automatically deleted

## Configuration options

| Option | Default | Description |
|---|---|---|
| Duration | 30 min | Meeting length |
| Buffer before | 0 min | Gap before the meeting |
| Buffer after | 0 min | Gap after the meeting |
| Minimum notice | 60 min | How far ahead the guest must book |
| Availability start | 09:00 | Earliest bookable time |
| Availability end | 17:00 | Latest bookable time |
| Days | Mon–Fri | Which days are bookable |

## Dashboard

Team links appear in a dedicated "Team links" section showing:

- Title and duration
- Member names
- **Copy link** — copies the public URL to clipboard
- **View** — opens the public slot page
- **Delete** — removes the link (only the creator can delete)

## Technical details

- **Availability computation**: Uses `BusySource::Team`, which requires all members' busy times to have no conflicts (intersection semantics). This is the opposite of `BusySource::Group` which uses union semantics (any member free).
- **CalDAV sync**: All members' calendars are synced on-demand when a guest visits the slot page (same stale-check as regular booking pages).
- **Busy time tracking**: Team link bookings are included in `fetch_busy_times_for_user`, so they correctly block availability for other booking flows.
