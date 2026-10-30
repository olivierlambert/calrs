# Lead Capture

Lead capture records what a guest types into a booking form *before* they submit it, so you can follow up on bookings that were started and not finished. It is off by default, twice over: the admin turns it on for the instance, and each event type opts in.

## What a guest sees

When capture is active for an event type, the slot page shows a small form (name and email, plus a phone number if the event type collects one for [SMS](./sms.md)) over a blurred calendar. The calendar opens once it is filled in. The gate is a convenience, not a lock: it runs in the browser, and a guest who skips it can still book.

Every public page of that event type carries a notice in its footer saying that what the guest types may be saved. If the admin set a **Legal notice URL**, the footer links to it instead of showing the inline text.

## Turning it on

1. **Admin → Lead capture**: switch capture on for the instance and choose how many days captured details are kept (1 to 365, default 30). A row is deleted that many days after its last update.
2. **Admin → Legal notice** (optional): the URL of your privacy page.
3. **Event type → Lead capture**: tick the box. The first time, you also confirm that you inform bookers their details are recorded. The date of that confirmation is kept with the event type.

You are responsible for informing bookers and for having a legal basis for recording what they type. calrs shows the notice and keeps the retention short; the rest is your policy.

## Working the leads

**Leads** in the sidebar lists the guests who started and did not finish: when, event type, name, email, phone, the slot they were looking at and where they came from (the `utm_source` and `utm_campaign` of the page URL, else the referrer's host). Four tiles count started, completed, open and the conversion rate.

Mark a lead as contacted, or archive it to take it off the list. Everyone sees the leads of their own event types and of the teams they belong to; admins see all of them. A lead turns into a completed one when the same guest finishes the booking.

## Alerts

About 30 minutes after a guest stopped typing, the host is emailed once with what was captured, in the host's language. Leads older than 48 hours are not alerted on, a host gets at most 10 alerts an hour, and nothing is sent while the instance has no SMTP or capture is switched off. The email needs an address that looks real; anything else is skipped.

## What is stored

Name, email, phone, a note, the slot, UTM parameters, the referrer, the guest's IP address and browser string. Each value is cut to a fixed length. The public endpoint is rate limited per IP, and one event type accepts at most 300 new leads an hour.
