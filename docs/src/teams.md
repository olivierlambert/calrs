# Teams

Teams allow multiple users to share booking pages with combined availability and automatic assignment.

## Key concepts

Teams replace the old separate "Groups" and "Team Links" concepts into a single unified system.

| Feature | Description |
|---|---|
| **Visibility** | **Public** (anyone can book) or **Private** (requires invite token) |
| **Scheduling mode** | **Round-robin** (any member free, assigned to least-busy) or **Collective** (all members must be free) |
| **Team admin** | Manages event types and settings without needing global admin |
| **OIDC sync** | Optionally link Keycloak groups — all group members become team members |

## Creating a team

From **Dashboard > Teams > + New**:

1. Set **name**, **slug**, and **description**
2. Choose **visibility**: public or private
3. Pick **members** from all enabled users
4. Optionally link **OIDC groups** (all group members become team members automatically)
5. Click **Create team**

The creator becomes a team admin. Global admins can remove themselves from teams they created — they retain management access via the admin panel. This supports the IT admin use case of creating teams without being bookable.

## Team settings

Any team admin can access settings from **Dashboard > Teams > Settings**:

- **Avatar upload** — team profile image
- **Description** — displayed on the public team page
- **Members** — view members and their roles
- **OIDC group linking** — from team settings, use the unified search bar to find and link OIDC groups. When a group is linked, all its members are automatically added to the team with source='group'. Members stay in sync on each OIDC login — new group members are added, removed members are cleaned up
- **Private teams** — the invite link is shown with a copy button for sharing
- **Time off** (read-only): the members' upcoming [time off](./event-types.md#time-off), each period with the member's name and its dates and times in your own time zone, so you can see who is away before planning a team event. The note a member gave a period is never shown. Only the team's admins see it, and the section is absent when no member has time off planned

## Team event types

Team event types are created from **Dashboard > Event Types > + New** (select the team from the dropdown) or from **Dashboard > Teams > team settings**.

> **Unified event types page:** Personal and team event types appear together in a single list on the **Event Types** dashboard page. Team event types are distinguished by a team name badge, so you can manage all your event types from one place.

They support the same options as personal event types:

- Duration, buffer before/after, minimum notice
- Availability schedule (days + hours)
- Calendar selection, location, confirmation mode
- Invite links (for private event types)

Where a team booking is written is each member's own choice: the booking lands in the assigned member's calendar (every member's for collective), so every member picks, under **Calendar sources > Bookings by event type**, which of their calendars receives the bookings of each team event type they belong to. Without a choice, the write calendar of their sources is used. See [CalDAV write-back](./caldav.md#per-event-type-write-confirmed-bookings-to).

Additional team-specific options:

- **Scheduling mode** — round-robin or collective (see below)
- **Member weights** — admins can set priority per member via the Member Priority card, which appears during both creation and editing. Group member weights provide a default; the event type can override the member priority. Weight 0 excludes a member from round-robin assignment for that event type — excluded members also don't appear on the public booking page's avatar list.

## Public team pages

- **Public teams:** `/team/{slug}` shows the team profile with avatar, description, members (faces with their first names), and event types. Each meeting says who the guest meets: "with one of" and the members' faces for round robin, "with everyone" for collective (members excluded with weight 0 are left out). Team admins group the team's meetings into sections and set their order at `/dashboard/teams/{id}/order`; the team's usual hours show only when its meetings share one timezone
- **Private teams:** `/team/{slug}?invite={token}` — same page, but requires a valid invite token
- **Slot picker:** `/team/{slug}/{event-slug}` — shows available slots based on the scheduling mode. The sidebar displays the team avatar and stacked member avatars (members excluded via weight 0 are hidden)
- **Legacy redirects:** `/g/{slug}` redirects to `/team/{slug}`, `/t/{token}` redirects to `/team/{slug}?invite={token}`

## Scheduling modes

### Round-robin

A slot is available if **any** team member is free. The booking is assigned by member priority, then workload (among equally prioritized available members, the one assigned the fewest bookings over the last 30 days).

When a booking is submitted:

1. calrs finds all team members (with weight > 0)
2. For each member, checks if the slot is free (no calendar events or bookings in the buffer window)
3. Among available members, prefers the highest configured priority, then the fewest bookings assigned over the last 30 days
4. The booking is assigned to that member and written to their configured calendar destination
5. If no member is available, the booking is rejected

**Best for:** support queues, sales demos, intake calls — any scenario where the guest doesn't care who they meet.

### Collective

A slot is available only if **all** team members are free. The booking includes every member.

When a booking is submitted:

1. calrs verifies all members are free for the slot
2. The booking is created and written to **each eligible** member's configured calendar destination
3. Email notifications are sent to all members
4. If any member has a conflict, the slot is not shown

**Best for:** panel interviews, group demos, team syncs with external guests.

### Excluding members

On a **collective** event type, per-member exclusions can be set from the event type editor — useful when a team member is joining the team but shouldn't (yet) be required for a specific event. Excluded members don't gate the availability window, don't receive notifications, and don't appear on the public booking page's avatar list. The event type's bookings don't make them busy either, so their own event types stay bookable at those times.

## Booking watchers

Designate a team as **watchers** on an event type to separate the "who gets booked" decision from the "who can pick up this booking" decision.

When a booking lands on a watched event type:

1. Every watcher team member gets an email with a **Claim this booking** button
2. The first watcher to click the button claims the booking — a short-lived token backs each button
3. Subsequent clicks land on an "already claimed" page; no double-assignment
4. Claimed bookings show up on the watcher's dashboard with a "Claimed by you" badge
5. A confirmed claimed booking makes the watcher busy: their own event types stop offering that time, and the availability troubleshooter lists it as a claimed booking

**Typical setup:** a customer self-serves through a public event type, and a rotating support team watches it. Whoever has bandwidth grabs the booking — no round-robin assumptions, no manual dispatch.

Configuration: in the event type editor, or on the create form once a team is picked, scroll to **Booking watchers** and pick one or more teams. Watchers can be set on any scheduling mode; they're independent from the round-robin / collective assignment.

## Multi-timezone teams

A team event type's weekly rules use its **explicit event timezone**, not the server's timezone. Each member's default working hours use that member's profile timezone. calrs converts the member's hours and calendar events into the event timezone before checking availability.

For example, give a Paris/New York support team a broad event schedule in `Europe/Paris`, then have each member set their actual hours in **Profile & Settings**. Round-robin offers a slot when an eligible member is within their own hours and free; collective requires every eligible member to be within their hours and free. Daylight-saving changes are applied using the named timezones.

Each member's [time off](./event-types.md#time-off), busy calendars and existing bookings also constrain their availability. A broad team schedule does not by itself make people available outside their personal hours. Guests view the resulting slots in their selected timezone.

## OIDC group sync

Groups synced from your OIDC provider can be linked to teams, automatically adding group members as team members.

### How it works

1. User logs in via SSO
2. calrs reads the `groups` claim from the JWT
3. Groups are created if they don't exist (leading `/` stripped from Keycloak paths)
4. User is added to their groups and removed from groups they no longer belong to
5. Groups linked to teams via the `team_groups` junction table sync membership automatically

OIDC-synced members get `role='member'`, never admin. Manual team admin status is preserved across syncs.

### Keycloak setup

In your Keycloak realm:

1. Create groups under **Groups** (e.g., "Sales", "Engineering")
2. Assign users to groups
3. Add a `groups` mapper to your client:
   - **Mapper type:** Group Membership
   - **Token claim name:** `groups`
   - **Add to ID token:** ON
   - **Full group path:** ON (calrs strips the leading `/`)

## Private teams vs internal vs private event types

There are three ways to restrict access to team bookings. They serve different use cases and can be combined:

| Mechanism | What it gates | Who distributes links | Use case |
|---|---|---|---|
| **Private team** | The entire team page | Team admin shares one invite link | Controlled distribution — only the team admin decides who books |
| **Internal event type** | A single event type (personal or team) | Any authenticated employee via Invite Links page (under Shared Links in the sidebar) | Self-serve — any Sales rep can generate a Support Call link for a customer |
| **Private event type** | A single event type | Event type owner sends personalized invites | Targeted — send invites to specific guests with pre-filled info |

### When to use each

**Private team** — your team handles external meetings but you don't want colleagues exposed to unsolicited bookings. The team admin shares the invite link only with approved contacts. Example: a consulting team where only the account manager shares the booking page with clients.

**Internal event type** — you (or your team) provide a cross-org service and you want any colleague to be a link distributor, without involving the owner each time. Works for both personal and team event types. Example: IT Help Desk, Support Calls, or a senior engineer's "Code Review" slot — any colleague can generate a one-time link from the Invite Links page (under Shared Links in the sidebar) and paste it in a Slack message or support ticket. Links are single-use and expire after 7 days.

**Private event type** — you want to send personalized invites to specific guests with their name and email pre-filled. Example: demo team sends targeted invites to qualified leads with custom messages.

### Combining them

- A **public team** can have **internal** event types — the team page is public but some event types are only bookable via employee-generated links
- A **private team** can have **internal** event types — guests need the team invite token first, then employees can generate per-event-type links
- A **public team** can have **private** event types — the team page lists public event types, but private ones require their own invite
- **Personal internal** event types work the same way — any colleague can generate links from the Invite Links page, but the booking is assigned to the event type owner (not round-robin)

## Dashboard

The **Teams** page in the dashboard shows all teams you belong to:

- Team avatar, name, and visibility badge (public/private)
- Member count
- **Settings** link (visible to team admins)
- Global admins see all teams and can create new ones
