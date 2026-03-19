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
- **Private teams** — the invite link is shown with a copy button for sharing

## Team event types

Team event types are created from **Dashboard > Event Types > + New** (select the team from the dropdown) or from **Dashboard > Teams > team settings**. Personal and team event types appear in a single unified list on the Event Types dashboard page — team event types are distinguished by a team name badge.

They support the same options as personal event types:

- Duration, buffer before/after, minimum notice
- Availability schedule (days + hours)
- Calendar selection, location, confirmation mode
- Invite links (for private event types)

Additional team-specific options:

- **Scheduling mode** — round-robin or collective (see below)
- **Member weights** — admins can set priority per member via the Member Priority card, which appears during both creation and editing. Weights can be set globally on the team or overridden per event type. Weight 0 excludes a member from round-robin assignment for that event type — excluded members also don't appear on the public booking page's avatar list.

## Public team pages

- **Public teams:** `/team/{slug}` — shows team profile with avatar, description, members, and event types
- **Private teams:** `/team/{slug}?invite={token}` — same page, but requires a valid invite token
- **Slot picker:** `/team/{slug}/{event-slug}` — shows available slots based on the scheduling mode. The sidebar displays the team avatar and stacked member avatars (members excluded via weight 0 are hidden)
- **Legacy redirects:** `/g/{slug}` redirects to `/team/{slug}`, `/t/{token}` redirects to `/team/{slug}?invite={token}`

## Scheduling modes

### Round-robin

A slot is available if **any** team member is free. The booking is assigned to the **least-busy available member** (fewest confirmed bookings).

When a booking is submitted:

1. calrs finds all team members (with weight > 0)
2. For each member, checks if the slot is free (no calendar events or bookings in the buffer window)
3. Among available members, picks the one with the fewest confirmed bookings
4. The booking is assigned to that member and pushed to their CalDAV calendar
5. If no member is available, the booking is rejected

**Best for:** support queues, sales demos, intake calls — any scenario where the guest doesn't care who they meet.

### Collective

A slot is available only if **all** team members are free. The booking includes every member.

When a booking is submitted:

1. calrs verifies all members are free for the slot
2. The booking is created and pushed to **every** member's CalDAV calendar
3. Email notifications are sent to all members
4. If any member has a conflict, the slot is not shown

**Best for:** panel interviews, group demos, team syncs with external guests.

## Multi-timezone teams

The availability window on a team event type (e.g., Mon-Fri 09:00-17:00) is defined once for the whole team and interpreted in the server's timezone. For teams spread across timezones, this window may not cover everyone's working hours.

**Recommended setup:** Set a wide availability window (e.g., 06:00-23:00 or even 00:00-23:59) and let each member's CalDAV calendar handle the actual blocking. Because calrs syncs each member's calendar independently and converts events from their original timezone, the slot picker naturally shows the correct availability:

- Alice (Paris, 09:00-17:00 CET) — her calendar blocks evenings and weekends
- Bob (New York, 09:00-17:00 EST) — his calendar blocks his mornings (CET afternoon/evening)
- A guest sees slots from 09:00-23:00 CET, with Alice covering the morning and Bob covering the evening

This approach requires no per-member configuration — just sync your calendars and set a wide window.

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
| **Internal event type** | A single event type | Any authenticated employee via Invite Links page (under Shared Links in the sidebar) | Self-serve — any Sales rep can generate a Support Call link for a customer |
| **Private event type** | A single event type | Event type owner sends personalized invites | Targeted — send invites to specific guests with pre-filled info |

### When to use each

**Private team** — your team handles external meetings but you don't want colleagues exposed to unsolicited bookings. The team admin shares the invite link only with approved contacts. Example: a consulting team where only the account manager shares the booking page with clients.

**Internal event type** — your team provides a cross-org service and you want any employee to be a link distributor, without involving the team admin each time. Example: IT Help Desk, Support Calls — any employee can generate a one-time link from the Invite Links page (under Shared Links in the sidebar) and paste it in a Slack message or support ticket. Links are single-use and expire after 7 days.

**Private event type** — you want to send personalized invites to specific guests with their name and email pre-filled. Example: demo team sends targeted invites to qualified leads with custom messages.

### Combining them

- A **public team** can have **internal** event types — the team page is public but some event types are only bookable via employee-generated links
- A **private team** can have **internal** event types — guests need the team invite token first, then employees can generate per-event-type links
- A **public team** can have **private** event types — the team page lists public event types, but private ones require their own invite

## Dashboard

The **Teams** page in the dashboard shows all teams you belong to:

- Team avatar, name, and visibility badge (public/private)
- Member count
- **Settings** link (visible to team admins)
- Global admins see all teams and can create new ones
