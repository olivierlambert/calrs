# Groups

Groups allow multiple users to share a single booking page with combined availability and automatic assignment.

## How groups work

- Groups are synced from your OIDC provider (e.g., Keycloak `groups` JWT claim)
- A group event type shows slots where **any** group member is free
- When a guest books, the booking is assigned to the **least-busy available member** (round-robin)
- Each member's CalDAV calendar and existing bookings are checked independently

## Group sync (OIDC)

Groups are created automatically from the `groups` claim in the OIDC ID token:

1. User logs in via SSO
2. calrs reads the `groups` claim from the JWT
3. Groups are created if they don't exist (leading `/` stripped from Keycloak paths)
4. User is added to their groups and removed from groups they no longer belong to

> Groups are only available with OIDC authentication. Local-only users cannot be in groups.

## Creating group event types

From the dashboard:

1. Click **+ New** under "Group event types"
2. Select the group from the dropdown (only groups you belong to are shown)
3. Fill in the event type details (same as individual event types)

## Public group pages

- **Group profile:** `/g/group-slug` — lists all enabled group event types
- **Slot picker:** `/g/group-slug/meeting-slug` — shows slots where any member is free
- **Booking:** `/g/group-slug/meeting-slug/book?date=...&time=...`

## Round-robin assignment

When a booking is submitted for a group event type:

1. calrs finds all group members
2. For each member, checks if the slot is free (no calendar events or bookings in the buffer window)
3. Among available members, picks the one with the **fewest confirmed bookings**
4. The booking is assigned to that member
5. If no member is available, the booking is rejected

## Dashboard

Group event types appear in a separate "Group event types" section on the dashboard, showing the group name and a link to the public page.

## Keycloak setup

In your Keycloak realm:

1. Create groups under **Groups** (e.g., "Sales", "Engineering")
2. Assign users to groups
3. Add a `groups` mapper to your client:
   - **Mapper type:** Group Membership
   - **Token claim name:** `groups`
   - **Add to ID token:** ON
   - **Full group path:** ON (calrs strips the leading `/`)
