# Webhooks

calrs can tell another system about your bookings. Add a webhook and calrs
sends a signed JSON message to its URL whenever a booking is made, requested,
approved, declined, expires, is cancelled or moves to another time. Use it to
feed a CRM, a ticketing system, a chat channel, n8n, Zapier or a script of
your own.

Webhooks are configured at **Event types → Webhooks**
(`/dashboard/webhooks`).

## Who can add one

By default only administrators can. A webhook sends guest names, email
addresses, phone numbers and notes to an address of the user's choosing, so
letting every user do that is a decision for the operator: **Admin panel →
Email and SMS → Booking webhooks → Let any user add webhooks**. Turning the
switch back off stops the webhooks other users added at once and drops the
deliveries queued for them. Their configuration is kept.

## Scope

Each webhook covers one of:

- **All my personal event types**: every event type you own, now and later.
  Team event types are not included. Only you see this webhook.
- **One event type**: a personal event type you own, or a team event type of
  a team you administer. Everyone who may manage that event type (the owner,
  the team's admins, instance admins) sees the webhook and can edit or remove
  it; the page shows who added it.

Up to 10 webhooks per scope.

A webhook only sends while the person who added it can still manage its
event type. When they cannot any more (removed from the team or no longer
one of its admins, the team deleted, their account disabled or deleted), the
webhook is **suspended**: kept, shown as suspended to the event type's
managers, and its queued deliveries dropped. A current manager can **Take
over** the webhook, which makes it theirs and resumes it. A webhook for "all
my personal event types" is deleted with its user.

## Choosing events

Each webhook has a checkbox per event, all ticked by default. Events left
unticked are not sent at all. With every box ticked the webhook also
receives events added in later versions of calrs.

## Events

| Event | When |
|---|---|
| `booking.created` | A booking is made and confirmed straight away. |
| `booking.requested` | A booking is made on an event type that requires approval. |
| `booking.confirmed` | The host approves a pending request, or a pending reschedule. |
| `booking.declined` | The host declines a pending request (or a pending reschedule). |
| `booking.expired` | A pending request reached its deadline unanswered. calrs declines it. |
| `booking.cancelled` | The guest or the host cancelled, or the host deleted the event from their calendar. `cancelled_by` says who. |
| `booking.rescheduled` | The booking moved. `previous` holds the old times; `status` is `pending` when the new time waits for approval. |
| `booking.claimed` | A member of a watcher team claimed the booking. `claimed_by` names them. |
| `ping` | The **Send test** button. `test` is `true`, the booking is made up. |

Bookings and cancellations made with the CLI (`calrs booking create`,
`calrs booking cancel`) are queued too and delivered by the running
`calrs serve`.

## The request

```
POST /your/endpoint HTTP/1.1
Content-Type: application/json
User-Agent: calrs-webhook/2.0.0
X-Calrs-Event: booking.created
X-Calrs-Delivery: 5b0f2c1e-8d0e-4a8e-9a43-6a1d1c7f3e21
X-Calrs-Signature: sha256=4f0c...e9
```

```json
{
  "version": 1,
  "id": "5b0f2c1e-8d0e-4a8e-9a43-6a1d1c7f3e21",
  "event": "booking.created",
  "occurred_at": "2026-11-07T09:12:44Z",
  "timestamp": 1794042764,
  "test": false,
  "booking": {
    "uid": "0b7d2e0e-41f1-4a43-8a8e-0c2b1f4a9d10@calrs",
    "status": "confirmed",
    "start": "2026-11-10T09:00:00Z",
    "end": "2026-11-10T09:30:00Z",
    "language": "fr",
    "created_at": "2026-11-07T09:12:44Z",
    "guest": {
      "name": "Jeanne Martin",
      "email": "jeanne@example.com",
      "phone": "+33612345678",
      "notes": "About the renewal",
      "timezone": "Europe/Paris"
    },
    "attendees": ["paul@example.com"],
    "hosts": [{ "name": "Alice", "email": "alice@example.com" }],
    "claimed_by": null,
    "location": { "type": "link", "value": "https://meet.example.com/abc" },
    "previous": null,
    "cancelled_by": null,
    "reason": null
  },
  "event_type": {
    "id": "6c1e4f0a-...",
    "slug": "intro",
    "title": "Intro call",
    "timezone": "Europe/Paris",
    "team": null
  }
}
```

- **`version`** is `1`. New fields may be added without a bump; a change that
  breaks a receiver gets a new version.
- **`id`** identifies the delivery and stays the same across retries (also in
  `X-Calrs-Delivery`). Store the ids you processed and ignore repeats: a
  delivery can arrive twice if your answer is lost.
- **`timestamp`** is the Unix time of this attempt, inside the signed body.
- **`occurred_at`** is when the change happened. Retries keep it.
- **Times** (`start`, `end`, `previous`) are UTC instants. `event_type.timezone`
  is the zone the event type is scheduled in, `guest.timezone` the one the
  guest booked in.
- **`booking.uid`** is the iCalendar UID of the invitation, stable for the
  life of the booking: use it to match later events to the first one.
- **`status`** is the status the event left the booking in: `confirmed`,
  `pending`, `cancelled` or `declined`. A `booking.created` that arrives
  after the booking was already cancelled still says `confirmed`; the
  cancellation follows it.
- **`claimed_by`** is `{ "name", "email" }` once a watcher claimed the
  booking, else `null`.
- **`hosts`** lists the host, the assigned member of a round-robin team, or
  every member of a collective team.
- **`guest.phone`** is set only when the event type asks for a phone number,
  in E.164 form. **`notes`**, **`phone`**, **`language`**, **`location`** and
  **`team`** can be `null`.
- **`reason`** is the cancellation or decline reason, the same text the guest
  is shown.
- **`team`** is `{ "id", "slug", "name" }` for a team event type.

Nothing else leaves calrs: no cancel, reschedule or approval tokens, no
credentials, no resource names, no calendar data.

## Verifying a request

The signature is HMAC-SHA256 of the raw request body, keyed with the webhook's
signing secret, hex encoded: the same scheme as the meeting-link webhook and
the SMS webhook gateway. Check it on the bytes you received, before parsing,
then refuse old timestamps so a captured request cannot be replayed.

Python:

```python
import hashlib, hmac, json, time

SECRET = b"the signing secret you entered in calrs"

def verify(body: bytes, signature_header: str) -> dict:
    expected = "sha256=" + hmac.new(SECRET, body, hashlib.sha256).hexdigest()
    if not hmac.compare_digest(expected, signature_header or ""):
        raise ValueError("bad signature")
    payload = json.loads(body)
    if abs(time.time() - payload["timestamp"]) > 300:
        raise ValueError("stale request")
    return payload
```

Node.js:

```js
const crypto = require("crypto");

function verify(rawBody, signatureHeader, secret) {
  const expected = "sha256=" +
    crypto.createHmac("sha256", secret).update(rawBody).digest("hex");
  const a = Buffer.from(expected), b = Buffer.from(signatureHeader || "");
  if (a.length !== b.length || !crypto.timingSafeEqual(a, b)) {
    throw new Error("bad signature");
  }
  const payload = JSON.parse(rawBody);
  if (Math.abs(Date.now() / 1000 - payload.timestamp) > 300) {
    throw new Error("stale request");
  }
  return payload;
}
```

Then drop a payload whose `id` you already handled.

## Delivery and retries

Webhooks never hold up or fail a booking. The change is saved first, the
delivery is queued, and a background task in `calrs serve` sends it, usually
within a second.

- Any 2xx answer counts as delivered. Anything else, a timeout (10 seconds)
  or a connection error is retried after 1, 5, 15 and 60 minutes, then 4 and
  12 hours: 7 attempts over about 17 hours. After that the delivery is given
  up and kept for a week, shown on the webhook's row, then deleted. A
  delivery still not sent after 24 hours (held back behind an earlier one,
  say) is given up the same way.
- The events of one booking are delivered in order: a later event waits until
  the earlier one is delivered or given up.
- Each webhook is served on its own: a slow or unreachable receiver never
  delays the others. While a receiver is failing, its other deliveries wait
  for the same retry, so it gets one request per retry, not one per booking.
- Each webhook shows the time and result of its last attempt, the error (with
  at most 200 characters of the response body, also while it is switched off;
  changing its URL clears them), and how many deliveries wait
  for a retry or were given up.
- **Send test** sends a `ping` straight away and shows the result, whatever
  events are ticked. Each user can send 10 tests per 10 minutes.
- **Switch off** stops deliveries and drops what is still queued, so switching
  back on starts from new events rather than replaying old ones. The same
  happens when a webhook is suspended, when its user is disabled, and, for
  webhooks added by non-admins, when the admin turns the switch off.

## Network rules

A webhook URL must be:

- `https://`. Plain `http://` is accepted only for a host on the operator's
  private-host allowlist (`CALRS_ALLOW_PRIVATE_HOSTS`, or **Admin panel →
  System**), for a receiver on the same private network.
- free of a user name or password: the signature authenticates calrs.
- on a public address. Hosts that resolve to loopback, private, link-local or
  carrier-grade NAT addresses are refused unless allowlisted, the same policy
  as CalDAV sources and resource feeds.

A private address and a host name that does not resolve get the same message
("this address cannot be used"), so the form cannot be used to find out which
internal names exist; the server log has the detail.

The URL is checked when it is saved and again before every delivery, and the
request connects to the addresses that were checked, so a DNS answer that
changes between the check and the connection cannot redirect it. Redirects are
not followed (a 3xx answer is a failure). Webhook requests ignore the
`HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` environment variables: a proxy
resolves the host itself, which would undo the address checks. Error messages
never include the URL, since it often carries a token.
