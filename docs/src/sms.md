# SMS Notifications

calrs can text the guest about their booking, in addition to email. The feature is opt-in twice over: an admin configures one SMS gateway for the instance, and each event type decides whether it asks guests for a phone number at all. With no gateway configured and every event type left alone, nothing changes anywhere in the booking flow.

Four gateways ship. Any other one can be reached through the generic webhook provider.

| Gateway | Notes |
|---|---|
| [Twilio](https://www.twilio.com/docs/sms/api) | The widest coverage, per-message billing |
| [GatewayAPI](https://gatewayapi.com/docs/apis/rest/) | Danish, EU region available at `gatewayapi.eu`, prepaid |
| [seven.io](https://docs.seven.io/en/rest-api/endpoints/sms) | German, prepaid |
| Generic webhook | calrs POSTs the message to a URL you control, so any gateway with an API can be bridged with a small script |

## What gets sent

Only to the guest, and only for four moments:

| Event | When |
|---|---|
| **Confirmed** | On booking, or when the host approves a booking that required confirmation |
| **Rescheduled** | When the meeting moves, by either side |
| **Cancelled** | When the host or the guest cancels |
| **Reminder** | Before the meeting, if the event type sets a reminder |

Hosts are not texted; there is no phone number on a user account. Messages are short by design and are translated into the language the guest booked in.

Sending is best-effort: a gateway outage, an unconfigured gateway, or a number the gateway refuses are all logged and never block a booking or fail a page.

## Configuring the gateway

Go to **Dashboard > Admin > SMS settings**. Pick a gateway and the form relabels itself for it, because the fields are named differently by each vendor.

| Field | Twilio | GatewayAPI | seven.io | Webhook |
|---|---|---|---|---|
| Account identifier | **Account SID** (required) | not used | not used | not used |
| Secret | **Auth token** | **API token** | **API key** | **HMAC secret** (optional) |
| Sender | **From number**, E.164, or an alphanumeric sender ID where the destination country allows one | up to 11 alphanumeric characters or 15 digits | up to 11 alphanumeric characters or 16 digits | passed through to your endpoint |
| Endpoint | optional, defaults to `https://api.twilio.com` | optional, defaults to `https://gatewayapi.com`; use `https://gatewayapi.eu` to keep traffic in the EU | optional, defaults to `https://gateway.seven.io` | **required**, your receiver's URL |

Two settings apply whichever gateway you pick:

- **Default country code** normalises phone numbers guests type in local form. A French guest typing `06 12 34 56 78` on an instance set to `+33` is stored as `+33612345678`. Numbers starting with `+` or `00` are taken as written, whatever this is set to.
- **Daily limit** caps messages per day across the whole instance. `0` means no limit. See [Keeping the bill bounded](#keeping-the-bill-bounded).

The secret is encrypted at rest (AES-256-GCM), like every other stored credential. It is never displayed again: leaving the field empty when saving keeps the current value. Switching to a different gateway does require entering that gateway's own credential, since the stored one belongs to the previous vendor.

### Testing it

**Test gateway** does one of two things:

- With a phone number, it sends a real message to it, which costs what a message costs.
- With the field left empty, it verifies your credentials without sending anything, for gateways that offer a way to do that (Twilio and seven.io do; GatewayAPI and the webhook do not, and will tell you to send a real test instead).

Failures report what the gateway actually said, normalised across vendors: rejected credentials, a refused recipient, a refused sender, insufficient credit, or rate limiting.

### Environment variables

Like SMTP, the whole configuration can come from the environment instead, which then takes precedence over the database and locks the admin form.

| Variable | Description |
|---|---|
| `CALRS_SMS_PROVIDER` | `twilio`, `gatewayapi`, `sevenio`, or `webhook` |
| `CALRS_SMS_API_KEY` | Account identifier (Twilio Account SID); leave unset for the others |
| `CALRS_SMS_API_SECRET` | The gateway credential |
| `CALRS_SMS_SENDER` | From-number or sender ID |
| `CALRS_SMS_BASE_URL` | Region or self-hosted endpoint; the target URL for the webhook provider |
| `CALRS_SMS_DEFAULT_COUNTRY_CODE` | e.g. `+33` |
| `CALRS_SMS_DAILY_CAP` | Messages per day, `0` for no limit |

The block is all-or-nothing: an incomplete set is ignored with a warning in the logs and calrs falls back to the database configuration, so a typo in a deployment unit cannot half-configure the gateway.

### Testing against a Twilio trial account

Twilio trial accounts refuse custom message bodies: `Body` has to carry the *name* of one of Twilio's predefined templates instead. Without a way around that, checking the Twilio path at all means holding a paid account, which is a lot to ask of someone who just wants to confirm that a booking reaches a phone.

Setting `CALRS_SMS_TWILIO_TRIAL=true` sends Twilio's `sms_appointment_reminders` template in place of the composed message. Everything else runs unchanged, so the credentials, the sender, the recipient normalisation, the response parsing, and all four booking events are exercised against the real API. The composed message is still built, and logged at debug level, so nothing about it goes untested.

This is a testing aid, not a deployment option:

- It is read from the environment only. There is no database column and no admin field, so it cannot be switched on from the panel and left on by accident.
- It sits outside the all-or-nothing `CALRS_SMS_*` block and is read on its own, so it works with a database-stored configuration too.
- The admin SMS card shows a warning while it is active, and every send logs one. All four events look identical on the handset once the template is substituted, so the log is the only place to tell them apart.
- **Test gateway** with the recipient left empty refuses outright if the variable is set on a full account. That is the one way this can cost money rather than save it: on a paid account the template name is just text, so a flag left set after an upgrade would text every guest the literal string `sms_appointment_reminders` at full price.
- Trial accounts only reach numbers verified in the Twilio console, up to five of them.

Unset the variable, or set it to anything other than `1`/`true`/`yes`/`on`, to go back to real message bodies.

Gateway-specific switches follow `CALRS_SMS_<PROVIDER>_<OPTION>`.

## Enabling SMS on an event type

Each event type has an **SMS notifications** setting with three values:

| Mode | Booking form | Effect |
|---|---|---|
| **Off** (default) | No phone field | No SMS, ever |
| **Optional** | Field shown, may be left empty | Guests who leave a number get texted; guests who don't simply get email |
| **Required** | Field shown and enforced | The booking cannot be submitted without a number |

Use **optional** for ordinary meetings, where a text is a convenience. Use **required** when the message is the point of the event type: a phone call, an on-site visit, anything where you need to reach the guest on that number.

The form tells guests in optional mode that leaving the field empty means no text messages, so nobody expects a reminder they will not get.

### Who may enable it

By default only global admins can put an event type into an SMS mode, because SMS spends credit on the gateway account the admin configured, and the booking form is public.

To open it up, tick **Let any user enable SMS on their event types** in the admin SMS card. Until you do, the setting is hidden from other users and their event types keep whatever an admin set. A user who cannot change the setting also cannot turn it off, so a member editing a team event type will not silently disable an admin's configuration.

## Phone numbers

Guests can type a number in whatever form is natural to them:

| Typed | Stored |
|---|---|
| `06 12 34 56 78` (instance default `+33`) | `+33612345678` |
| `0033612345678` | `+33612345678` |
| `+33 6 12 34 56 78` | `+33612345678` |
| `5551234567` (instance default `+1`) | `+15551234567` |

Spaces, dashes, dots, slashes and parentheses are ignored. A single leading `0` is treated as a national trunk prefix and dropped; countries that do not use one are unaffected.

The field carries a country picker, formats the number as it is typed, and validates it with libphonenumber, so a guest sees an inline field error rather than losing their form on submit. The server validates again regardless, and the gateway remains the only thing that truly knows whether a number can receive a message.

### Which country is preselected

The picker starts on a country so most guests never touch it:

1. The browser's language, but **only when it names a country**. `fr-FR` selects France and `pt-BR` selects Brazil.
2. Otherwise the **default country code** from your SMS settings.

A bare language tag such as `fr`, `pt` or `sv` is deliberately ignored, because a language is not a country. Swedish would read as El Salvador, `pt` would send Brazilian guests to Portugal, and plenty of people run an English browser wherever they live. Guessing from that would quietly rewrite a local number into a valid number belonging to a stranger, at your expense.

Because the flag is visible and the guest can change it, a preselection that does not suit them is a default they can see rather than a silent rewrite. No geo-IP lookup is made, and the widget is served from your own instance, so the page contacts nobody else.

Stored numbers are shown to the host on the bookings dashboard and are never shown to other guests. They are kept on the booking, so deleting a booking deletes the number.

## Keeping the bill bounded

Your booking page is public and the recipient number comes from whoever fills the form. That combination is the target of a known attack: **SMS pumping**, also called artificially inflated traffic, where someone submits bookings with numbers on expensive routes and takes a cut of the traffic charges. The same shape of abuse can be used to send unwanted messages to a third party's phone.

Four controls, and you want all of them:

1. **Restrict destination countries at your gateway.** Twilio calls this Messaging Geo Permissions; disable every country you do not serve. GatewayAPI and seven.io have equivalent destination controls. This is free, it happens before calrs is involved, and it removes the expensive-route incentive entirely.
2. **Keep the gateway account prepaid, without auto-recharge.** Whatever goes wrong then costs at most the float on the account.
3. **Set a daily limit** in the admin SMS card. Past it, calrs stops texting and keeps sending email, so bookings keep working while the spend stops. Today's count and cost are shown in the same card.
4. **Leave the captcha on.** An SMS-enabled booking form without a captcha is an open relay someone else pays for. The admin panel warns you when SMS is configured and the captcha is not.

Booking endpoints are also rate limited per IP (10 requests per 5 minutes), which bounds the rate but not a distributed attempt, so it is not a substitute for the four above.

## Message content and cost

Messages are billed per segment: 160 characters for the GSM-7 alphabet, but only 70 if the text contains a single character outside it, which includes most accented letters. calrs keeps its messages inside two segments in every shipped language, and shortens long event titles so the date and time always survive.

The gateway reports segments and cost where it can, and calrs records them so the admin panel can show today's spend. That usage ledger stores no phone numbers.

## Using the generic webhook

Pick **Generic webhook** and give it a URL. On each message calrs sends:

```http
POST https://your-endpoint.example.com/sms
Content-Type: application/json
X-Calrs-Signature: sha256=<hex>

{"to": "+33612345678", "text": "Booking confirmed: ...", "sender": "calrs"}
```

Any 2xx response counts as accepted. If your receiver answers with `{"id": "..."}`, that id is kept in the logs.

The signature header is only sent when an HMAC secret is configured, and is the hex-encoded HMAC-SHA256 of the raw request body. Verify it to prove the call came from your calrs instance.

The webhook URL is deliberately not subject to the private-host protection used for CalDAV, because pointing it at a bridge on localhost is the main reason to use it.

## Troubleshooting

**Nothing is sent.** Check, in order: the gateway is configured and enabled, the event type is not in **Off** mode, the guest actually left a number, and today's count is below the daily limit. Each of these is logged when it stops a message.

**"Switching SMS gateway requires entering that gateway's credential."** You changed the gateway in the dropdown but left the secret field empty. The stored secret belongs to the previous vendor, so it cannot be carried over.

**Messages arrive from a strange sender.** Alphanumeric sender IDs are not permitted in every country, and some networks rewrite them. Check your gateway's rules for the destinations you send to.

**A guest cannot submit the booking form.** In **Required** mode a number is mandatory. If they insist their number is valid and calrs disagrees, check the instance default country code: a national-format number is interpreted against it.
