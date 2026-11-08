# Booking Questions

An event type can ask guests more than their name and email: a licence plate
before a tyre change, the topics of a first consultation, whether they agree
to the call being recorded. The questions are set per event type, in the
**Booking form** section of its edit page, by whoever may edit that event type
(its owner, a team's admins, instance admins).

## Kinds of question

| Kind | The guest sees | Stored |
|---|---|---|
| Short text | one line, up to 500 characters | the text |
| Long text | a text area, up to 5000 characters | the text, line breaks kept |
| One choice | radio buttons, or a drop-down above five choices | the chosen choice |
| Several choices | one checkbox per choice | the chosen choices |
| Checkbox | one box to tick | yes or no |

Each question can be **required** or optional, and has an optional help text
shown under it. Questions appear on the booking form in the order you give
them (Move up, Move down), after the name, email and SMS phone fields and
before the notes.

A required checkbox must be ticked, which is what a privacy agreement needs.
An optional checkbox starts unticked, and its "no" is kept too: "did not agree
to the recording" matters as much as "agreed".

Mark a checkbox as **consent** when the guest agrees to something (a privacy
policy, a recording, a newsletter), and give it a **policy link** if there is
a document to read: the link is shown under the box and opens in a new tab.
calrs keeps, with the booking, the sentence the guest read, the link, whether
the box was ticked and when the form was sent, along with the guest's email
and the booking's language. It does not keep IP addresses or browser details.
A consent box is never ticked in advance.

Limits: 20 questions per event type, 30 choices per question (200 characters
each), 200 characters per question (1000 for a checkbox, whose label is the
sentence agreed to), 500 for a help text. A policy link must be a full
`https://` or `http://` address.

### The notes field

The free-text notes field stays. **Notes field** in the same section sets it
to optional (as before), required, or hidden when the questions already ask
everything.

## Where the answers appear

| | Answers |
|---|---|
| Bookings page of the dashboard, the booking's notes page | yes, folded under the booking |
| Host emails: new booking, approval request and its reminder, booking confirmed, host reminder, reschedule request | yes, after the notes |
| The event calrs writes to the host's calendar (CalDAV, Exchange, Google, a watcher's copy once claimed) | yes, except questions with **Write the answer into the calendar event** turned off, and except the one Google copy that lists the attendees (Google shows its description to all of them), as for the guest's phone |
| The guest's confirmation page, confirmation and pending emails | yes, their own answers |
| Webhooks and the REST API | yes, see [Webhooks](webhooks.md) and [REST API](api.md) |
| The emailed invitation (`.ics`), the "Add to calendar" file, additional attendees' emails, reminders and cancellations to the guest, SMS, email subjects, shared resource calendars, logs | never |

Turn **Write the answer into the calendar event** off for sensitive answers:
assistants, delegates and anyone a calendar is shared with can read the
event's description. The answer still reaches the host by email and on the
dashboard.

Watchers of a team event type see the answers once they have claimed the
booking, as they see the notes.

## Editing questions later

Answers keep the question as the guest read it. Renaming a question, changing
its choices or deleting it changes future bookings only: past bookings still
show what the guest was asked and answered. A saved question keeps its kind;
to change it, remove the question and add a new one.

Each question gets a stable **key**, made from its label when it is created
(shown in the editor). Webhooks use it, and it does not change when the
question is renamed.

**Duplicate** copies the questions and the notes setting along with the rest
of the event type.

## Languages

Questions, help texts and choices are shown as you write them, in one
language. A guest whose browser is in another language reads calrs' own
labels in theirs and your questions in yours. "Yes", "No" and "You agreed"
are written in each reader's language.

## Rescheduling and invites

A rescheduled booking keeps its answers; the guest is not asked again.
Invite links ask the same questions. Bookings created with
`calrs booking create` ask none: the host is the one booking. A booking
through the REST API asks none either, unless it sends `answers`, keyed by
question key; then they are checked like the form's, required questions
included, and stored the same way (see [REST API](api.md)).
