# Booking Flow

## Guest experience

1. **Visit the booking page** — `/u/host/meeting-slug` (or via an invite link for private event types)
2. **Pick a timezone** — auto-detected from the browser, changeable via dropdown
3. **Browse available slots** — displayed as a week view, navigate with Previous/Next buttons
4. **Click a slot** — opens the booking form
5. **Fill in details** — name, email, optional notes (pre-filled from invite if applicable)
6. **Add guests** — optionally invite additional attendees (if the event type allows it)
7. **Submit** — booking is created
8. **Confirmation page** — shows booking summary (including any additional attendees)
9. **Email** — guest and any additional attendees receive a confirmation email with an `.ics` calendar invite attached

![Available time slots](images/slots.png)

![Booking form](images/booking-form.png)

## Booking statuses

| Status | Description |
|---|---|
| `confirmed` | Booking is active. Slot is blocked. Emails sent. |
| `pending` | Awaiting host approval (when `requires_confirmation` is on). |
| `cancelled` | Cancelled by host or guest. Slot is freed. |
| `declined` | Declined by host (pending booking rejected). |

## Confirmation mode

When an event type has **requires confirmation** enabled:

1. Guest submits booking → status is `pending`
2. Guest receives a "pending" email (no `.ics` yet)
3. Host receives an "approval request" email with **Approve** and **Decline** buttons
4. Host can approve/decline in two ways:
   - **From the email** — click the Approve or Decline button (no login required, token-based)
   - **From the dashboard** — go to **Pending approval** section and click Confirm or Decline
5. On confirm: status becomes `confirmed`, guest receives confirmation email with `.ics`, booking is pushed to CalDAV
6. On decline: status becomes `declined`, guest receives a decline notification with optional reason

> **Note:** The email action buttons require `CALRS_BASE_URL` to be set. Without it, the host must use the dashboard.

## Cancellation

From the dashboard, click **Cancel** on an upcoming booking:

1. Optionally enter a reason
2. Confirm the cancellation
3. Both guest and host receive cancellation emails with a `METHOD:CANCEL` `.ics` attachment
4. If the booking was pushed to CalDAV, the event is deleted from the calendar

### Guest self-cancellation

Guests can cancel their own bookings via a link in the confirmation email:

1. Click the "Cancel booking" link in the email
2. Optionally enter a reason
3. Confirm the cancellation
4. Both guest and host are notified

The cancellation email correctly attributes who cancelled (host vs guest).

## Reschedule

Bookings can be rescheduled without cancelling and rebooking. Both guests and hosts can initiate a reschedule.

### Guest reschedule

Guests can reschedule their booking via the reschedule link in the confirmation or pending email:

1. Click the "Reschedule" button in the email
2. Pick a new time slot from the slot picker (the current booking's slot is freed so it remains available)
3. Confirm the new time
4. The booking moves to `pending` status — the host must approve via email or dashboard
5. If the booking was previously pushed to CalDAV, the event is removed (re-pushed on approval)

### Host reschedule

Hosts can reschedule from the dashboard:

1. Go to **Dashboard > Bookings** and click **Reschedule** on a booking
2. Pick a new time slot
3. Confirm the new time
4. The booking stays `confirmed` — no approval needed
5. The CalDAV event is updated in place (same UID)
6. The guest receives a reschedule notification with the updated `.ics` invite

### Token regeneration

After each reschedule, the `reschedule_token`, `cancel_token`, and `confirm_token` are regenerated. This invalidates any previous email links, ensuring only the latest links work.

### Edge cases

- **Already cancelled/declined bookings** cannot be rescheduled (error page shown)
- **Self-conflict** is handled: the booking being rescheduled doesn't block its own new slot
- **Group bookings** keep the original `assigned_user_id` (no re-running round-robin)
- **Reminder state** is reset: `reminder_sent_at` is cleared so a new reminder is sent for the updated time

## Conflict detection

Before a booking is accepted, calrs checks for conflicts:

- **Calendar events** — from synced CalDAV sources
- **Existing bookings** — confirmed bookings on any event type
- **Buffer times** — the buffer before/after is included in the conflict window
- **Minimum notice** — slots too close to the current time are rejected

Additionally, a database-level unique index prevents two bookings from occupying the same slot, even if two guests submit simultaneously.

## CalDAV write-back

When a booking is confirmed (either directly or via approval), calrs can push the event to the host's CalDAV calendar. See [CalDAV Integration > Write-back](./caldav.md#caldav-write-back) for setup.

## Email notifications

If SMTP is configured, calrs sends emails at these moments:

| Event | Guest receives | Host receives |
|---|---|---|
| Booking confirmed | Confirmation + `.ics` REQUEST | Notification + `.ics` REQUEST |
| Booking pending | "Awaiting confirmation" notice | Approval request with Approve/Decline buttons |
| Booking declined | Decline notice (with optional reason) | — |
| Booking cancelled | Cancellation + `.ics` CANCEL | Cancellation + `.ics` CANCEL |
| Booking rescheduled (by host) | Reschedule notification + updated `.ics` | — |
| Reschedule request (by guest) | "Pending" notice with updated time | Reschedule approval request with Approve/Decline buttons |
| Booking reminder | Reminder with cancel button | Reminder with details |
| Invite sent | Invite email with booking link | — |

All emails are sent as **HTML with plain text fallback**. They include event title, date, time, timezone, location, and notes. The HTML templates are responsive and support dark mode in email clients that honor `prefers-color-scheme`.

## Timezone handling

- Guest's timezone is auto-detected via `Intl.DateTimeFormat` in the browser
- A timezone dropdown lets the guest change it
- Slots are displayed in the guest's selected timezone
- The booking is stored in the host's timezone
- The timezone is preserved across navigation (week picker, booking form)

## CLI booking

```bash
calrs booking create intro \
  --date 2026-03-20 --time 14:00 \
  --name "Jane Doe" --email jane@example.com \
  --timezone Europe/Paris --notes "Let's discuss the project"
```
