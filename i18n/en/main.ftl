# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Booking pending
confirmed-page-title-booked = Booking confirmed

confirmed-heading-reschedule-requested = Reschedule requested
confirmed-heading-rescheduled = Rescheduled!
confirmed-heading-pending = Pending confirmation
confirmed-heading-booked = You're booked!

confirmed-subtitle-reschedule-requested = Your reschedule request has been sent to { $host }. You'll receive an email at { $email } once it's approved.
confirmed-subtitle-rescheduled = Your booking has been rescheduled. A confirmation email has been sent to { $email }.
confirmed-subtitle-pending = Your booking request has been sent to { $host }. You'll receive an email at { $email } once it's confirmed.
confirmed-subtitle-booked = A confirmation email has been sent to { $email }.

confirmed-detail-event = Event:
confirmed-detail-date = Date:
confirmed-detail-time = Time:
confirmed-detail-with = With:
confirmed-detail-location = Location:
confirmed-detail-notes = Notes:
confirmed-detail-additional-guests = Additional guests:

confirmed-book-another = Book another time

confirmed-add-to-calendar = Add to calendar

# Slot picker (templates/slots.html)

slots-location-video = Video call
slots-location-phone = Phone call
slots-location-google-meet = Google Meet

slots-tz-label = Your timezone
slots-time-format-label = Time format

slots-view-month = Month view
slots-view-week = Week view
slots-view-column = Column view

slots-weekday-mon = Mon
slots-weekday-tue = Tue
slots-weekday-wed = Wed
slots-weekday-thu = Thu
slots-weekday-fri = Fri
slots-weekday-sat = Sat
slots-weekday-sun = Sun

slots-weekday-mon-short = M
slots-weekday-tue-short = T
slots-weekday-wed-short = W
slots-weekday-thu-short = T
slots-weekday-fri-short = F
slots-weekday-sat-short = S
slots-weekday-sun-short = S

slots-select-date = Select a date
slots-loading-availability = Loading availability...
slots-click-highlighted = Click a highlighted date to see available times
slots-no-times-month = No available times this month
slots-no-times-day = No available times this day
slots-no-availability-participants = No availability found for all participants this month
slots-week-more = more

# Booking form (templates/book.html)

book-page-title = Book { $title }
book-back-to-times = Back to times
book-name-label = Your name
book-name-placeholder = Jane Doe
book-email-label = Email
book-email-placeholder = jane@example.com
book-email-invalid = Please enter a complete email address, including the domain (e.g. jane@example.com).
book-notes-label = Notes
book-notes-optional = (optional)
book-notes-placeholder = Anything you'd like to discuss?
book-additional-guests-label = Additional guests
book-additional-guests-hint = (optional, up to { $max })
book-add-guest-btn = + Add guest email
book-guest-email-placeholder = colleague@example.com
book-phone-label = Phone number
book-phone-placeholder = 06 12 34 56 78
book-phone-help = Local numbers are fine; { $country } is assumed unless you start with +.
book-phone-optional-consequence = Leave it empty if you would rather not get text messages about this booking.
book-phone-required = A phone number is required for this booking.
book-phone-invalid-title = Invalid phone number
book-phone-invalid = Please enter a phone number we can text, or leave the field empty.
book-phone-country-search = Search
book-phone-country-label = Select country
book-phone-country-none = No country selected
book-phone-country-no-results = No countries match that search
captcha-label = Security verification
captcha-initial-state = Verify you're human
captcha-verifying = Verifying...
captcha-solved = You're human
captcha-error = Error
captcha-troubleshooting = Troubleshooting
captcha-wasm-disabled = Enable WASM for significantly faster solving
captcha-verify-aria = Click to verify you're a human
captcha-verifying-aria = Verifying, please wait
captcha-verified-aria = Verified
captcha-required = Please verify you're human
captcha-error-aria = An error occurred, please try again
book-confirm-button = Confirm booking

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Booking confirmed: { $event }, { $date } at { $time } ({ $tz }).
sms-cancelled = Booking cancelled: { $event }, { $date } at { $time } ({ $tz }).
sms-rescheduled = Booking moved: { $event } is now { $date } at { $time } ({ $tz }).
sms-reminder = Reminder: { $event } starts { $date } at { $time } ({ $tz }).

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Guest:
common-detail-reason = Reason:
common-reason-optional = (optional)
common-close-page = You can close this page.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Cancel booking
cancel-heading = Cancel booking
cancel-subtitle = You are about to cancel your booking.
cancel-reason-label = Reason
cancel-reason-placeholder-host = Let the host know why...
cancel-button = Cancel booking
cancelled-heading = Booking cancelled
cancelled-subtitle = Your booking has been cancelled and the host has been notified.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Decline booking
decline-heading = Decline booking
decline-subtitle = You are about to decline this booking request.
decline-reason-placeholder-guest = Let the guest know why...
decline-button = Decline booking
declined-heading = Booking declined
declined-subtitle = The booking has been declined and the guest has been notified.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Approve booking
approve-heading = Approve booking
approve-subtitle = You are about to approve this booking request.
approve-button = Approve booking
approved-heading = Booking approved
approved-subtitle = The booking has been confirmed and a confirmation email has been sent to { $email }.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Claim booking
claim-heading = Claim booking
claim-subtitle = You are about to claim this booking. You will be added as an attendee.
claim-assigned-to = Assigned to:
claim-button = Claim this booking
claimed-page-title = Booking claimed
claimed-heading = Booking claimed
claimed-subtitle = You have claimed this booking. A calendar invite has been sent to your email.
already-claimed-page-title = Already claimed
already-claimed-heading = Already claimed
already-claimed-subtitle = This booking has already been claimed by { $name }.

# Generic error page (booking_action_error.html)

action-error-page-title = Booking action error

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Reschedule booking — calrs
host-resched-heading = Reschedule booking
host-resched-subtitle = This will send { $guest } an email asking them to pick a new time.
host-resched-currently = Currently:
host-resched-button = Send reschedule request
host-resched-cancel-link = Cancel

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Confirm reschedule
resched-confirm-heading = Confirm reschedule
resched-confirm-subtitle = You are about to move your booking to a new time.
resched-was = Was:
resched-new = New:
resched-button = Confirm reschedule
resched-back-to-picker = Back to time picker

# Base layout chrome (templates/base.html)

base-loader-checking = Checking availability
base-loader-please-wait = Please wait, loading the latest calendar data...
base-stop-impersonating = Stop impersonating
base-theme-toggle = Toggle theme
base-powered-by = Powered by

# Profile (templates/profile.html)

profile-pick-event-type-invite = Pick an event type to book a time.
profile-no-event-type = No event types available yet.

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

common-month-1 = January
common-month-2 = February
common-month-3 = March
common-month-4 = April
common-month-5 = May
common-month-6 = June
common-month-7 = July
common-month-8 = August
common-month-9 = September
common-month-10 = October
common-month-11 = November
common-month-12 = December

common-weekday-long-mon = Monday
common-weekday-long-tue = Tuesday
common-weekday-long-wed = Wednesday
common-weekday-long-thu = Thursday
common-weekday-long-fri = Friday
common-weekday-long-sat = Saturday
common-weekday-long-sun = Sunday

# Format patterns are parametric per locale to handle word order. Translators
# pick where each placeholder lands. Example outputs:
#   EN: April 2026  /  Tuesday, March 12, 2026
#   FR: avril 2026  /  mardi 12 mars 2026
#   ES: abril 2026  /  martes, 12 de marzo de 2026
common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday }, { $month } { $day }, { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Reschedule
email-action-cancel-booking = Cancel booking

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
email-confirm-greeting = Hi { $name },
email-confirm-headline = Your booking has been confirmed!
email-confirm-ics-attached-plain = A calendar invite is attached.
email-confirm-ics-attached-html = A calendar invite is attached to this email.
email-confirm-need-to-cancel = Need to cancel? { $url }

# Email: guest reminder

email-reminder-subject = Reminder: { $event } at { $time }
email-reminder-headline = Your meeting is coming up.

# Email: guest cancellation

email-cancel-subject = Cancelled: { $event } — { $date }
email-cancel-headline-by-host = Your booking has been cancelled by { $host }.
email-cancel-headline-by-guest = Your booking has been cancelled.
email-cancel-ics-attached-plain = A calendar cancellation is attached.
email-cancel-ics-attached-html = A calendar cancellation is attached to this email.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Note: cancellation requires at least { $minutes } minutes notice.
email-confirm-reschedule-notice = Note: rescheduling requires at least { $minutes } minutes notice.

# Event type form: Google Meet location + cancel/reschedule minimum notice
# (templates/event_type_form.html, src/google_meet.rs)

event-type-form-location-google-meet = Google Meet (auto-generated link)
event-type-form-location-google-meet-hint = A unique Google Meet link is created on confirmation, owned by the assigned host. Every host (you, or every eligible team member) must have Google Calendar connected with a write-back calendar selected.

google-meet-prereq-no-host = Google Meet requires a host with Google Calendar connected.
google-meet-prereq-no-eligible = Google Meet requires at least one eligible team member with Google Calendar connected.
google-meet-prereq-missing = Google Meet requires every host to have Google Calendar connected with a write-back calendar selected. Still missing: { $names }. Connect them at Dashboard → Calendar sources.
google-meet-unavailable-title = Google Meet is not available
google-meet-dynamic-group-unavailable = The host needs Google Calendar connected with a write-back calendar selected.

event-type-form-cancel-notice-label = Minimum notice to cancel
event-type-form-reschedule-notice-label = Minimum notice to reschedule
event-type-form-notice-help = Leave empty for no restriction.
event-type-form-resources-label = Required resources
event-type-form-resources-hint = Slots are offered only when the selected resources are available, according to the mode below.
event-type-form-resources-mode-all = All selected resources must be free
event-type-form-resources-mode-round-robin = Any one free resource is enough (it gets assigned to the booking)
event-type-form-notice-unit-minutes = minutes
event-type-form-notice-unit-hours = hours
event-type-form-notice-unit-days = days
event-type-form-booking-horizon-label = Booking horizon
event-type-form-booking-horizon-help = How many days ahead guests can book. Leave empty for no limit, 0 for today only.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = Cancellation requires at least { $minutes } minutes notice before the meeting.
confirmed-reschedule-notice-info = Rescheduling requires at least { $minutes } minutes notice before the meeting.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = This booking can no longer be cancelled online
booking-blocked-title-reschedule = This booking can no longer be rescheduled online
booking-blocked-body = The host requires at least { $minutes } minutes of notice. If you cannot attend, please email <a href="mailto:{ $host_email }">{ $host_email }</a> directly.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Copy
dashboard-event-types-copied = Copied!
dashboard-event-types-copy-title = Copy booking link
dashboard-event-types-copy-failed = Copy failed

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Scheduling
nav-overview = Overview
nav-event-types = Event Types
nav-bookings = Bookings
nav-teams = Teams
nav-section-shared-links = Shared Links
nav-invite-links = Invite Links
nav-section-calendars = Calendars
nav-sources = Sources
nav-section-personal = Personal
nav-settings = Profile & Settings
nav-troubleshoot = Troubleshoot
nav-section-admin = Admin
nav-admin-panel = Admin Panel
nav-sign-out = Sign out
nav-release-notes = View release notes

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = Your browser timezone is { $detected } but your booking timezone is set to { $current }.
tz-banner-update = Update
tz-banner-dismiss = Dismiss

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = Enter URL:
editor-link-default-label = link text
editor-placeholder-text = text
editor-nothing-to-preview = Nothing to preview

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Dashboard
overview-welcome = Welcome, { $name }
overview-public-page = Public page:
overview-avail-banner-title = Default availability
overview-avail-banner-body = Your default working hours have been set to Mon–Fri, 9:00–17:00. These are used when others include you in dynamic group meetings.
overview-avail-banner-cta = Review your availability
overview-dismiss = Dismiss
overview-getting-started = Getting started
overview-getting-started-help = Follow these steps to start accepting bookings.
overview-step-connect-calendar = Connect a calendar
overview-step-first-event-type = Create your first event type
overview-step-share-link = Share your booking link
overview-pending-approval = Pending approval
overview-booking-with = { $title } with { $guest }
overview-badge-pending = pending
overview-guest-booked = Guest booked:
overview-confirm = Confirm
overview-decline = Decline
overview-stat-event-types = Event Types
overview-stat-upcoming = Upcoming Bookings
overview-stat-pending = Pending Approval
overview-stat-sources = Calendar Sources
overview-quick-actions = Create a new event type
overview-action-public-title = Public booking page
overview-action-public-desc = Share a link — anyone can pick a slot and book time with you.
overview-action-team-title = Team scheduling
overview-action-team-desc = Distribute bookings across team members or find a time everyone's free.
overview-action-team-desc-empty = Create a team first, then set up shared event types.
overview-action-private-title = Private invite-only
overview-action-private-desc = Generate single-use links for specific contacts. No one else can book.
overview-action-shared-title = Shared invite links
overview-action-shared-desc = Any colleague in the team can generate booking links to share externally.
overview-action-reason-calendar = Connect a calendar first
overview-action-reason-ask-admin = Ask an admin to create a team
overview-action-reason-team-admin = Requires a team — create one first
overview-action-reason-team-member = Requires a team — ask an admin

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Bookings
bookings-pending-approval = Pending approval
bookings-available-to-claim = Available to claim
bookings-upcoming = Upcoming bookings
bookings-with = { $title } with { $guest }
bookings-guest-booked = Guest booked:
bookings-resource = Resource:
bookings-confirm = Confirm
bookings-reschedule = Reschedule
bookings-decline = Decline
bookings-claim = Claim
bookings-badge-awaiting-reschedule = awaiting reschedule
bookings-cancel = Cancel
bookings-reason-placeholder = Reason (optional)
bookings-confirm-cancel = Confirm cancel
bookings-back = Back
bookings-empty = No upcoming bookings yet.<br>Share your { $link } so people can book time with you.
bookings-empty-link-label = event type links

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Teams
teams-heading = Teams
teams-new = New
teams-badge-public = public
teams-badge-private = private
teams-settings = Settings
teams-view = View
teams-empty = No teams yet.
teams-empty-admin = { $link } to collaborate with your team.
teams-empty-admin-link-label = Create one
teams-empty-member = Teams are created by administrators. Ask your admin to create one and add you as a member.

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Invite Links
invite-links-heading = Invite Links
invite-links-new = New internal event
invite-links-help = Generate single-use booking links for internal event types. Any authenticated colleague can create and share links here.
invite-links-duration = { $minutes }min
invite-links-hosted-by = Hosted by { $host }
invite-links-get-link = Get link
invite-links-invites = Invites
invite-links-empty = No internal event types yet.<br>{ $link } with "Internal" visibility to let any colleague generate booking links.
invite-links-empty-link-label = Create an event type
invite-links-js-generating = Generating...
invite-links-js-copied = Copied!
invite-links-js-error = Error

teams-member-count =
    { $count ->
        [one] { $count } member
       *[other] { $count } members
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Calendar Sources
sources-heading = Calendar sources
sources-add = Add
sources-last-sync = Last sync:
sources-sync = Sync
sources-full-resync = Full resync
sources-full-resync-title = Clear cache and re-fetch all events from server
sources-test = Test
sources-reconnect = Reconnect
sources-reconnect-title = Re-run the Google consent flow
sources-edit = Edit
sources-remove = Remove
sources-remove-confirm = Remove source '{ $name }'? This will delete all synced events from this source.
sources-no-write-calendar = No write calendar selected. Confirmed bookings stay in calrs and are not pushed to this calendar. Pick one below to enable write-back.
sources-write-bookings-to = Write bookings to:
sources-write-none = None (don't write)
sources-empty = No calendar sources connected. { $link } to check availability.
sources-empty-link-label = Add one

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Event Types
event-types-heading = Event types
event-types-new = New
event-types-badge-disabled = disabled
event-types-badge-internal = internal
event-types-badge-private = private
event-types-badge-resources = resources
event-types-send-invites = Send invites
event-types-duration = { $minutes }min
event-types-mode-collective = collective
event-types-mode-round-robin = round robin
event-types-edit = Edit
event-types-disable = Disable
event-types-enable = Enable
event-types-embed = Embed
event-types-overrides = Overrides
event-types-team-settings = Team settings
event-types-invites = Invites
event-types-view-public = View public page
event-types-view-page = View page
event-types-delete = Delete
event-types-delete-confirm = Delete event type '{ $title }'? This cannot be undone.
event-types-empty = No event types yet. { $link } to start accepting bookings.
event-types-empty-link-label = Create one

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Bold (Ctrl+B)
editor-italic = Italic (Ctrl+I)
editor-strikethrough = Strikethrough
editor-code = Inline code
editor-link = Insert link (Ctrl+K)
editor-toggle-preview = Toggle preview
editor-preview = Preview

# Profile and settings (templates/settings.html)

settings-page-title = Settings
settings-heading = Profile & Settings
settings-public-page-label = Your public booking page
settings-copy = Copy
settings-copied = Copied!
settings-open = Open
settings-avatar = Avatar
settings-upload = Upload
settings-remove = Remove
settings-display-name = Display name
settings-display-name-placeholder = Your name
settings-username = Username
settings-username-hint = (used in your booking URL)
settings-username-pattern-title = Lowercase letters, numbers, and dashes only
settings-username-help = Your public booking page:
settings-title = Title
settings-title-placeholder = e.g. Software Engineer, Product Manager
settings-title-help = Shown on your public profile and in the sidebar.
settings-bio = Bio
settings-bio-placeholder = Tell people a bit about yourself...
settings-bio-help = Shown on your public booking page. Supports **bold**, *italic*, ~~strikethrough~~, `code`, and [links](url).
settings-booking-email = Booking email
settings-booking-email-help = This email will appear on your public booking pages and in email notifications. Leave empty to use your login email.
settings-booking-email-warning = Make sure this email exists on your mail provider. If it doesn't, notifications won't be delivered.
settings-timezone = Timezone
settings-timezone-help = Your availability rules and booking times are computed in this timezone.
settings-language = Language
settings-language-auto = Auto (browser default)
settings-language-help = Pick a UI language, or leave on Auto to follow your browser's setting.
settings-dynamic-group = Allow others to include me in dynamic group links
settings-dynamic-group-help = When enabled, other users can create ad-hoc collective meeting URLs that include you (e.g. { $example }).
settings-lend-resource = Lend my calendar access for resource reservations
settings-lend-resource-help = When a booking needs to reserve a shared resource (demo lab, meeting room) that your calendar account can write to, allow calrs to use your stored calendar credentials for that write.
settings-default-availability = Default availability
settings-default-availability-help = Your default working hours. Used for dynamic group links when others include you in a meeting.
settings-copy-to-all = Copy to all days
settings-copy-to-all-title = Copy the first enabled day's windows to all other enabled days
settings-add-window = Add time window
settings-remove-window = Remove window
settings-save = Save settings
settings-appearance = Appearance
settings-theme-system = System
settings-theme-light = Light
settings-theme-dark = Dark

# Sign in (templates/auth/login.html)

login-page-title = Sign in
login-heading = Sign in
login-subtitle = Sign in to your calrs account
login-sso = Sign in with SSO
login-or = or
login-email = Email
login-password = Password
login-submit = Sign in with email
login-no-account = Don't have an account? { $link }
login-register-link = Register

# Registration (templates/auth/register.html)

register-page-title = Register
register-heading = Create account
register-subtitle = Register for a new calrs account
register-domains-limited = Registration is limited to: { $domains }
register-name = Name
register-name-placeholder = Your name
register-email = Email
register-password = Password
register-password-hint = (min. 12 characters)
register-submit = Create account
register-have-account = Already have an account? { $link }
register-signin-link = Sign in

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Too many login attempts. Please try again later.
auth-error-invalid-credentials = Invalid email or password
auth-error-internal = Internal error
auth-error-registration-disabled = Registration is disabled.
auth-error-name-length = Name must be between 1 and 255 characters
auth-error-email-length = Email must be between 1 and 255 characters
auth-error-email-invalid = Please enter a valid email address
auth-error-email-domain = Email domain not allowed
auth-error-password-length = Password must be at least 12 characters
auth-error-email-taken = Email already registered
auth-error-create-failed = Failed to create account

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Calendar source
source-test-sync-heading = Sync: { $name }
source-test-heading = Connection test
source-write-page-title = Set up calendar write-back
source-write-back = Back to dashboard
source-write-heading = Where should bookings go?
source-write-help = When someone books a meeting with you, calrs can automatically create the event in your calendar. Pick which calendar to write bookings to for { $name }.
source-write-save = Save
source-write-skip = Skip for now
source-write-sync-results = Sync results

source-write-event-count =
    { $count ->
        [one] { $count } event
       *[other] { $count } events
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Date overrides
overrides-heading = Date overrides
overrides-back-teams = Back to teams
overrides-back-event-types = Back to event types
overrides-intro = Add date-specific exceptions for { $title }
overrides-add-heading = Add new override
overrides-date = Date
overrides-type = Override type
overrides-type-blocked = Block entire day
overrides-type-custom = Custom hours
overrides-start-time = Start time
overrides-end-time = End time
overrides-add-submit = Add override
overrides-existing = Existing overrides
overrides-badge-blocked = blocked
overrides-badge-custom = custom hours
overrides-delete = Delete
overrides-delete-confirm = Delete this override?
overrides-empty = No date overrides yet.<br>Use the form above to block specific dates (holidays, days off) or set custom hours.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Pick an event type to book a time.
team-profile-empty = No event types available yet.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Troubleshoot
troubleshoot-empty = No event types found. { $link } to start troubleshooting availability.
troubleshoot-empty-link-label = Create one
troubleshoot-subtitle = See why time slots are available or blocked for { $title }
troubleshoot-duration = { $minutes }min
troubleshoot-buffer-before = { $minutes }min buffer before
troubleshoot-buffer-after = { $minutes }min buffer after
troubleshoot-min-notice = { $minutes }min notice
troubleshoot-blocked-override = Blocked by date override (day off)
troubleshoot-custom-hours-active = Custom hours override active (replaces weekly rules)
troubleshoot-legend-available = Available
troubleshoot-legend-calendar-event = Calendar event
troubleshoot-legend-booking = Booking
troubleshoot-legend-resource = Resource busy
troubleshoot-legend-outside = Outside hours
troubleshoot-legend-buffer = Buffer / Min. notice
troubleshoot-blocked-slots = Blocked slots
troubleshoot-none-date-blocked = This date is blocked by an availability override (day off). No slots available.
troubleshoot-none-custom-hours = Custom hours override active but no matching windows. Check your override settings.
troubleshoot-none-no-rules = No availability rules for this day of the week. This event type is not bookable on { $date }.
troubleshoot-none-all-bookable = No blocked slots during availability hours. All times are bookable.
troubleshoot-label-outside = Outside availability
troubleshoot-label-available = Available
troubleshoot-label-min-notice = Min. notice ({ $minutes }min)
troubleshoot-label-beyond-horizon = Beyond booking horizon ({ $days } days)
troubleshoot-label-buffer = Buffer ({ $minutes }min)
troubleshoot-label-resource-busy = Resource busy: { $names }
troubleshoot-detail-around = Around: { $label }
troubleshoot-detail-around-booking = Around: { $guest } booking
troubleshoot-reason-calendar-event = Calendar event: { $label }
troubleshoot-reason-booking = Booking: { $label }

# Invite management (templates/invite_form.html)

invites-heading = Invites
invites-back-teams = Back to teams
invites-back-event-types = Back to event types
invites-intro = Send invite links for { $title }
invites-capped = <strong>Input was capped at { $max } recipients per submission.</strong> Submit the rest in another batch.
invites-failed-hint = — check server logs for details.
invites-quick-link = Quick link
invites-quick-link-help = Generate a single-use link and copy it to your clipboard.
invites-get-link = Get link
invites-or-email = Or send via email
invites-recipients = Recipients
invites-recipients-hint = (one email per line, max { $max })
invites-message = Personal message
invites-message-hint = (optional, sent to every recipient)
invites-message-placeholder = Looking forward to showing you a demo...
invites-expires-in = Expires in
invites-expires-days = { $days } days
invites-expires-never = Never
invites-allow-multiple = Allow multiple bookings per recipient
invites-send = Send invites
invites-sent-heading = Sent invites
invites-badge-expired = expired
invites-badge-used = used
invites-badge-active = active
invites-sent-by = Sent by { $name }
invites-uses = { $used }/{ $max } uses
invites-expires-at = Expires { $date }
invites-copy-link = Copy link
invites-delete = Delete
invites-delete-confirm = Delete this invite?
invites-empty = No invites sent yet. Use the form above to send a booking link to someone.
invites-js-generating = Generating...
invites-js-copied = Copied!
invites-js-error = Error

invites-sent-count =
    { $count ->
        [one] Sent { $count } invite.
       *[other] Sent { $count } invites.
    }

invites-skipped-invalid =
    { $count ->
        [one] Skipped { $count } invalid row:
       *[other] Skipped { $count } invalid rows:
    }

invites-skipped-duplicate =
    { $count ->
        [one] Skipped { $count } duplicate row:
       *[other] Skipped { $count } duplicate rows:
    }

invites-failed =
    { $count ->
        [one] { $count } invite failed (DB or SMTP):
       *[other] { $count } invites failed (DB or SMTP):
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Edit calendar source
source-form-title-add = Add calendar
source-form-heading-edit = Edit calendar source
source-form-heading-add = Connect a calendar
source-form-subtitle-edit = Update the connection. Leave the password blank to keep the existing one. After changing the URL or username, run a sync to refresh the discovered calendar list.
source-form-subtitle-add = Connect a CalDAV server or Microsoft Exchange (EWS) so calrs can check availability when guests book meetings.
source-form-backend = Backend
source-form-preset = Preset
source-form-connect-google = Connect with Google
source-form-google-unavailable = Google Calendar is not available. Contact your administrator.
source-form-name = Display name
source-form-name-placeholder = My Calendar
source-form-url-caldav = CalDAV URL
source-form-url-ews = EWS endpoint URL
source-form-username = Username
source-form-password = Password
source-form-password-keep = Leave blank to keep existing
source-form-password-placeholder = App password or account password
source-form-skip-test = Skip connection test
source-form-skip-test-help = Use this if the test hangs (common with some BlueMind/Zimbra setups). You can test the connection later.
source-form-save = Save changes
source-form-add = Add calendar source
source-form-help-google-configured = Click the button below to authorize calrs to access your Google Calendar.
source-form-help-google-unconfigured = Google Calendar integration is not configured yet. Ask your administrator to set up Google OAuth2 credentials in the admin panel.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — Use the DAV endpoint of your BlueMind server.<br> Typically: <code>https://mail.yourcompany.com/dav/</code><br> Username is your <strong>email address</strong> (e.g. <code>alice@yourcompany.com</code>), not just the login name.<br> If the connection test hangs, check "Skip connection test" and try syncing directly.
source-form-help-nextcloud = <strong>Nextcloud</strong> — Use the WebDAV root, not a specific calendar URL.<br> Typically: <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — Use your full email in the URL path.<br> Example: <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Use an app-specific password (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — Use <code>https://caldav.icloud.com/</code><br> You need an app-specific password from <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Security &rarr; App-Specific Passwords).
source-form-help-zimbra = <strong>Zimbra</strong> — Use the DAV endpoint of your Zimbra server.<br> Typically: <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — Use the SOGo DAV endpoint.<br> Typically: <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — Use the server root URL.<br> Typically: <code>https://cal.example.com/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong>. Use the SOAP endpoint:<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> Username is the mailbox email; password must accept HTTP Basic over TLS (enable on a service mailbox if your tenant disabled Basic).<br> Make sure to also pick <strong>Microsoft Exchange (EWS)</strong> in the Backend dropdown above.
source-form-help-google = <strong>Google Calendar</strong>: Connect via OAuth2. No password needed.<br>
source-form-help-other = Enter your CalDAV server's <strong>DAV root URL</strong> — not a specific calendar or public link.<br> calrs will auto-discover your calendars via PROPFIND (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Bold
editor-italic-short = Italic
editor-link-short = Insert link

# Team creation (templates/team_form.html)

team-form-heading = New team
team-form-name = Team name
team-form-name-placeholder = Engineering
team-form-slug = Slug
team-form-slug-hint = (URL-friendly identifier)
team-form-slug-pattern-title = Lowercase letters, numbers, and dashes only
team-form-description = Description
team-form-optional = (optional)
team-form-description-placeholder = What this team is about...
team-form-description-help = Shown on the team page. Supports **bold**, *italic*, and [links](url).
team-form-visibility = Visibility
team-form-public = Public
team-form-private = Private
team-form-visibility-help = Private teams get an invite token for sharing. Public teams are visible on the team profile page.
team-form-members = Members
team-form-members-help = You will be added as team admin automatically. Add individual users or link OIDC groups.
team-form-search-placeholder = Search users or groups...
team-form-search-users = Users
team-form-search-groups = OIDC Groups
team-form-you = (you)
team-form-submit = Create team

# Team settings (templates/team_settings.html)

team-settings-page-title = Settings
team-settings-subtitle = Team settings — team admins can edit these.
team-settings-public-url = Public URL
team-settings-public-url-help = Anyone can book via this link.
team-settings-invite-link = Invite link
team-settings-invite-link-help = Share this link to give people access to this private team's booking page.
team-settings-avatar = Team avatar
team-settings-profile = Profile
team-settings-description-placeholder = Tell people about this team...
team-settings-description-help = Shown on the team's public booking page. Supports **bold**, *italic*, and [links](url).
team-settings-visibility-help = Public teams are listed on the team profile page. Private teams require an invite link to access.
team-settings-members-help = Manage who belongs to this team. Add individual users or link OIDC groups for automatic sync.
team-settings-role-member = Member
team-settings-role-admin = Admin
team-settings-oidc-group = OIDC group
team-settings-remove = Remove
team-settings-save = Save changes
team-settings-danger-zone = Danger zone
team-settings-danger-help = Permanently delete this team. Event types will be unlinked (not deleted). This cannot be undone.
team-settings-delete = Delete this team
team-settings-delete-confirm = Delete team '{ $name }'? This cannot be undone.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Edit event type
etf-heading-new = New event type
etf-team = Team
etf-team-hint = (optional — leave empty for personal event type)
etf-team-personal = Personal
etf-scheduling-mode = Scheduling mode
etf-mode-round-robin = Round Robin — assign to one available member
etf-mode-collective = Collective — all members must be available
etf-scheduling-mode-help = Round Robin assigns to one available member (least busy first). Collective requires all members to be free at the same time.
etf-title = Title
etf-title-placeholder = 30min intro call
etf-slug = Slug
etf-slug-placeholder = auto-generated from title
etf-description-placeholder = A quick introductory call to discuss...
etf-description-help = Shown on the booking page. Supports **bold**, *italic*, and [links](url).
etf-location = Location
etf-location-link = Video call (static URL)
etf-location-jitsi = Jitsi (auto-generated room)
etf-location-webhook = Webhook (custom provider)
etf-location-phone = Phone
etf-location-in-person = In person
etf-location-custom = Custom
etf-location-details = Details
etf-location-details-placeholder = https://meet.example.com/my-room
etf-pattern-placeholder = Leave empty to use the org default pattern
etf-duration = Duration (minutes)
etf-slot-interval = Slot interval (minutes)
etf-slot-interval-placeholder = Same as duration
etf-slot-interval-help = How often slots start. Leave blank to match duration.
etf-required-members = Required members
etf-required-members-help = All checked members must be free for a slot to be offered. Uncheck members you want to exclude (their availability will be ignored).
etf-member-priority = Member Priority
etf-member-priority-help = Higher priority members are assigned bookings first when available. Same priority = balanced by recent booking count.
etf-member-timezone-title = Member's timezone. Their personal working hours are interpreted in this TZ.
etf-priority-high = High
etf-priority-medium = Medium
etf-priority-low = Low
etf-section-availability = Availability
etf-timezone-help = The hours below are interpreted in this timezone. For team event types, pick the team's working timezone (not necessarily the creator's).
etf-reset-default = Reset to my default
etf-reset-default-title = Replace these hours with your profile-default availability
etf-availability-prefilled = Pre-filled from your { $link }. You can override it here for this event type.
etf-availability-prefilled-link = default availability
etf-section-buffers = Buffers & notice
etf-buffer-before = Buffer before (min)
etf-buffer-after = Buffer after (min)
etf-min-notice = Minimum notice
etf-min-notice-help = How far in advance someone must book.
etf-section-limits = Booking limits
etf-first-slot-only = One slot per day
etf-first-slot-only-help = Only show the earliest available time each day.
etf-freq-limit = Limit booking frequency
etf-freq-limit-help = Limit how many times this event can be booked per period.
etf-add-limit = Add limit
etf-section-options = Booking options
etf-requires-confirmation = Requires confirmation
etf-requires-confirmation-help = Bookings will be pending until you approve them from the dashboard.
etf-sms = SMS notifications
etf-sms-off = Off, no phone number asked
etf-sms-optional = Optional, guests may leave a number
etf-sms-required = Required, guests must leave a number
etf-sms-help = Texts the guest when their booking is confirmed, moved, cancelled, or about to start, in addition to email. Guests who leave the field empty simply get no SMS. Requires an SMS gateway in the { $link }.
etf-admin-panel-link = admin panel
etf-additional-guests = Additional guests
etf-guests-none = Guests cannot add others
etf-additional-guests-help = Allow the person booking to invite additional attendees who will receive the calendar invite.
etf-default-view = Default calendar view
etf-view-month = Month — calendar grid with slot list
etf-view-week = Week — 7-day columns with time slots
etf-view-column = Column — days listed with inline slots
etf-view-week-short = week
etf-view-column-short = column
etf-default-view-help = The view guests see by default. They can switch views anytime.
etf-conflict-calendars = Conflict calendars
etf-conflict-calendars-help = Select which calendars to check for conflicts. If none selected, all calendars are used.
etf-no-resources = No shared resources configured yet. Add one (demo lab, meeting room) in the { $link } to require it here.
etf-section-access = Access & notifications
etf-visibility-public = Public — visible on your profile
etf-visibility-internal = Internal — any colleague can generate invite links
etf-visibility-private = Private — invite link only
etf-visibility-help = Controls who can see and book this event type.
etf-vis-internal = Internal
etf-reminder = Booking reminder
etf-reminder-none = No reminder
etf-reminder-help = Send a reminder email to both you and your guest before the meeting.
etf-dynamic-group = Dynamic Group Link
etf-dynamic-group-help = Create an ad-hoc meeting link that checks availability for you and other users.
etf-dynamic-group-search = Search for a user to add...
etf-dynamic-group-note = Only users who allow dynamic group links are shown.
etf-dynamic-group-url = Group link URL
etf-watcher-teams = Watcher teams
etf-watcher-teams-help = Selected teams will be notified when bookings are made. Members can claim bookings to join as an attendee.
etf-save = Save changes
etf-create = Create event type
etf-js-loading = Loading...
etf-js-no-default = No default set
etf-js-reset-done = Reset!
etf-js-error = Error
etf-js-remove-limit = Remove limit
etf-period-day = Per day
etf-period-week = Per week
etf-period-month = Per month
etf-period-year = Per year

# Event type form: runtime summary hints (templates/event_type_form.html)


# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = No days set
etf-hint-every-day = Every day
etf-fmt-day-one = %1 day
etf-fmt-day-other = %1 days
etf-fmt-hours = %1h
etf-fmt-minutes = %1min
etf-hint-buffer-both = %1min before, %2min after
etf-hint-buffer-before = %1min buffer before
etf-hint-buffer-after = %1min buffer after
etf-hint-notice = %1 notice
etf-hint-no-buffers = No buffers, book anytime
etf-hint-max = Max %1
etf-hint-period-day = /day
etf-hint-period-week = /week
etf-hint-period-month = /month
etf-hint-period-year = /year
etf-hint-no-limits = No limits
etf-hint-confirmation-required = Confirmation required
etf-hint-auto-confirmed = Auto-confirmed
etf-hint-extra-guests-one = up to %1 extra guest
etf-hint-extra-guests-other = up to %1 extra guests
etf-hint-view = %1 view
etf-hint-reminder = reminder %1 before
etf-hint-no-reminder = no reminder

etf-guests-up-to =
    { $count ->
        [one] Up to { $count } additional guest
       *[other] Up to { $count } additional guests
    }

etf-reminder-hours =
    { $count ->
        [one] { $count } hour before
       *[other] { $count } hours before
    }

etf-reminder-days =
    { $count ->
        [one] { $count } day before
       *[other] { $count } days before
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Creating a <strong>public</strong> event type &mdash; anyone with the link can book.
etf-preset-private = Creating a <strong>private</strong> event type &mdash; only people you invite can book.
etf-preset-internal = Creating an <strong>internal</strong> event type &mdash; any colleague can share the booking link.
etf-preset-team = Creating a <strong>team</strong> event type &mdash; bookings are distributed across team members.
etf-pattern-hint = Optional pattern override. Tokens: <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Leave empty to use the organisation default configured by an admin.
etf-pattern-random-warning = This pattern has no <code>{"{"}random{"}"}</code> token. Two bookings of this event type on the same day will share the same room and the second guest can walk into the first guest's meeting. Use stable rooms only if that's what you want.
etf-webhook-hint = Per-booking meeting URL is fetched from the webhook configured by an admin under Admin &rarr; Meeting webhook. No URL needed here.

# Admin panel (templates/admin.html)

admin-page-title = Admin
admin-heading = Admin dashboard
admin-action-refused = Action refused:
admin-logo = Company logo
admin-logo-help = Displayed on public booking pages. Recommended: PNG or SVG, max 2 MB.
admin-company-link = Company link
admin-company-link-help = Logo links to this URL on public booking pages. Leave empty for no link.
admin-theme = Theme
admin-theme-help = Choose a color theme for all pages. The dark/light toggle is separate — themes adapt to both modes.
admin-theme-default = Default
admin-theme-default-desc = Clean blue
admin-theme-nord-desc = Arctic frost
admin-theme-dracula-desc = Dark purple
admin-theme-gruvbox-desc = Retro warm
admin-theme-solarized-desc = Ethan's classic
admin-theme-tokyo-desc = Neon cityscape
admin-theme-custom = Custom
admin-theme-custom-desc = Your colors
admin-custom-colors = Custom colors
admin-color-accent = Accent
admin-color-accent-hover = Accent hover
admin-color-bg = Background
admin-color-surface = Surface
admin-color-text = Text
admin-save-theme = Save theme
admin-users = Users ({ $count })
admin-user-filter = Filter by name or email…
admin-badge-admin = admin
admin-badge-disabled = disabled
admin-impersonate = Impersonate
admin-demote = Demote
admin-promote = Promote
admin-disable = Disable
admin-enable = Enable
admin-delete = Delete
admin-no-users-match = No users match your filter.
admin-no-users = No users yet.
admin-groups = Groups ({ $count })
admin-group-filter = Filter by group name…
admin-group-name = Group name
admin-weight = weight:
admin-no-groups-match = No groups match your filter.
admin-no-groups = No groups synced yet. Groups are automatically synced from your OIDC provider.
admin-auth-settings = Auth settings
admin-registration-enabled = Registration enabled
admin-allowed-domains = Allowed email domains
admin-allowed-domains-hint = (comma-separated, leave empty for any)
admin-save-auth = Save auth settings
admin-system-settings = System settings
admin-base-url = Base URL
admin-base-url-help = Public URL of this instance. Used for OIDC redirects and links in emails (approve/decline, cancel, reminders).
admin-private-hosts = Private-host allowlist
admin-private-hosts-help = Comma-separated hostnames allowed to resolve to private/reserved IPs for CalDAV/EWS sources (SSRF guard opt-out). Only add hosts you control (e.g. a calendar server on the same Docker network). Leave empty to keep the guard active for all hosts.
admin-unset-env = Unset the environment variable to edit this from here.
admin-save-system = Save system settings
admin-status = Status:
admin-status-enabled = enabled
admin-status-disabled = disabled
admin-status-disabled-paren = (disabled)
admin-status-configured = configured
admin-status-not-configured = not configured
admin-via-environment = (via environment)
admin-issuer = Issuer:
admin-client-id = Client ID:
admin-instance = Instance:
admin-oidc-settings = OIDC settings
admin-oidc-enabled = OIDC enabled
admin-issuer-url = Issuer URL
admin-client-id-label = Client ID
admin-client-secret = Client secret
admin-keep-current-hint = (leave empty to keep current)
admin-keep-current-set-hint = (leave empty to keep current — currently set)
admin-keep-unchanged = Leave empty to keep unchanged
admin-oidc-auto-register = Auto-register new users from OIDC
admin-save-oidc = Save OIDC settings
admin-google = Google Calendar (OAuth2)
admin-save-google = Save Google OAuth2 settings
admin-captcha = Captcha
admin-instance-url = Instance URL
admin-site-key = Site key
admin-secret = Secret
admin-widget-url = Widget script URL
admin-widget-url-help = Override if the CDN is blocked. Changes take effect immediately after saving.
admin-captcha-disable-help = Leave instance URL, site key and secret empty to disable captcha on booking pages.
admin-save-captcha = Save captcha settings
admin-resources = Resources
admin-resources-help = Shared bookable resources (demo lab, meeting rooms) backed by a calendar feed. Attached to event types, a busy resource blocks bookings.
admin-resource-stats = Events cached: { $events } &middot; Attached to { $attached } event type(s)
admin-never = never
admin-resource-sync-failed = (last attempt failed: { $error })
admin-writeback-enabled = Write-back: enabled ({ $via })
admin-writeback-readonly = Write-back: read-only
admin-teams-allowed = Teams allowed:
admin-teams-allowed-none = none (global admins only)
admin-sync-now = Sync now
admin-test-write = Test write
admin-delete-resource-confirm = Delete this resource? Event types using it will stop checking it.
admin-name = Name
admin-name-help = Leave empty to fetch the name from the feed.
admin-feed-url = ICS feed URL (publish address)
admin-feed-url-help = BlueMind: the resource calendar's public or private calendar address.
admin-caldav-url = CalDAV collection URL (for write-back)
admin-caldav-url-help = Optional. For BlueMind it is derived automatically from the feed URL.
admin-caldav-username = CalDAV username
admin-caldav-password = CalDAV password
admin-resource-teams = Teams allowed to use this resource
admin-resource-teams-help = Team admins of these teams can attach this resource to their team event types. Empty: global admins only.
admin-no-teams = No teams yet.
admin-save-resource = Save resource
admin-add-resource = Add resource
admin-jitsi = Jitsi (auto-generated meeting links)
admin-jitsi-help = When an event type's location is set to "Jitsi (auto-generated room)", calrs builds a fresh room URL per booking by appending the pattern below to your Jitsi base URL. No external API call is needed.
admin-display-name = Display name
admin-jitsi-display-name-placeholder = e.g. Meet DYB
admin-jitsi-display-name-help = Shown to guests on the slot picker and booking form. Defaults to "Video call" if empty.
admin-room-pattern = Room name pattern
admin-jitsi-disable-help = Leave the base URL empty to disable Jitsi auto-generation.
admin-save-jitsi = Save Jitsi settings
admin-meeting-webhook = Meeting webhook (bring-your-own provider)
admin-webhook-url = Webhook URL
admin-webhook-display-name-placeholder = e.g. Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Shown to guests instead of the generic "Video call" badge.
admin-authentication = Authentication
admin-auth-none = None
admin-auth-hmac = HMAC-SHA256 (X-Calrs-Signature header)
admin-shared-secret = Shared secret
admin-webhook-disable-help = Leave the URL empty to disable the meeting webhook.
admin-save-webhook = Save webhook settings
admin-smtp = SMTP settings
admin-smtp-test-sent = Test email sent.
admin-smtp-test-failed = Test email could not be sent. Check the server logs and your SMTP settings.
admin-smtp-env-error = SMTP environment configuration error:
admin-smtp-host = Host:
admin-smtp-from = From:
admin-smtp-enabled = SMTP enabled
admin-host = Host
admin-port = Port
admin-tls-mode = TLS mode
admin-tls-starttls = STARTTLS (port 587)
admin-tls-implicit = Implicit TLS (port 465)
admin-tls-none = None, unencrypted (local MTA only)
admin-smtp-username-hint = (leave empty for an unauthenticated relay)
admin-from-email = From email
admin-from-name = From name
admin-save-smtp = Save SMTP settings
admin-send-test-email = Send a test email to
admin-send-test-email-hint = (defaults to your account email)
admin-send-test-email-btn = Send test email
admin-smtp-clear-confirm = Delete the database SMTP configuration?
admin-clear-db-config = Clear database config
admin-sms = SMS settings
admin-sms-help = Optional. SMS is only sent for bookings on event types where "SMS notifications" is turned on, and only when the guest left a phone number.
admin-sms-test-sent = Test message sent.
admin-sms-test-checked = Credentials accepted.
admin-sms-test-error = The SMS gateway refused the request.
admin-sms-captcha-warning = The booking form is public and the recipient number comes from the guest, so SMS without a captcha is an open relay someone else can bill you for. Configure the captcha above, and restrict destination countries in your gateway's own settings.
admin-sms-sent-today = Sent today:
admin-sms-of-cap = of { $cap }
admin-sms-config-error = SMS configuration error:
admin-sms-gateway = Gateway:
admin-sms-account = Account:
admin-sms-sender = Sender:
admin-sms-enabled = SMS enabled
admin-sms-gateway-label = Gateway
admin-required-on-switch = Required when switching gateway
admin-sms-docs = { $provider } API documentation
admin-sms-country = Default country code
admin-sms-country-hint = (used when guests enter a local phone number)
admin-sms-daily-cap = Daily limit
admin-sms-daily-cap-hint = (messages per day for the whole instance, 0 for no limit)
admin-sms-daily-cap-help = Past the limit calrs stops texting and keeps sending email, so bookings never fail because the SMS budget ran out.
admin-save-sms = Save SMS settings
admin-send-test-sms = Send a test message to
admin-send-test-sms-hint-check = (leave empty to only check the credentials)
admin-send-test-sms-hint-e164 = (E.164 format)
admin-test-gateway = Test gateway
admin-sms-clear-confirm = Delete the database SMS configuration?
admin-sms-allow-all = Let any user enable SMS on their event types
admin-sms-allow-all-help = Off by default: SMS spends credit on the account configured here, so only admins may switch an event type into an SMS mode.
admin-save-policy = Save policy
admin-page-of = Page %1 of %2
admin-show-more-js = Show %1 more
admin-show-fewer = Show fewer

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Permanently delete user { $email }?{"\u000A"}{"\u000A"}This removes their user record, scheduling account, calendar sources, event types, and all data uniquely owned by them. Past bookings will be deleted with their event types.{"\u000A"}{"\u000A"}For OIDC/SSO users: if auto-register is enabled, this person will be re-created on their next login.{"\u000A"}{"\u000A"}This cannot be undone.
admin-system-settings-help = Public URL and network-security settings. These can also be set with the environment variables <code>CALRS_BASE_URL</code> and <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. When an environment variable is set it <strong>takes precedence</strong> over the value below.
admin-set-by-env = — set by environment ({ $var }), overriding the stored value
admin-google-help = To enable Google Calendar integration, create OAuth2 credentials at <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Enable the <strong>Google Calendar API</strong>, then add { $redirect_uri } as an authorized redirect URI.
admin-room-pattern-help = Available tokens: <code>{"{"}username{"}"}</code> (host), <code>{"{"}event{"}"}</code> (event type slug), <code>{"{"}date{"}"}</code> (YYYYMMDD), <code>{"{"}random{"}"}</code> (8 chars). Default: { $default }.
admin-room-pattern-warning = Without <code>{"{"}random{"}"}</code> the room name is predictable: two guests booking the same event type on the same day end up sharing a room and can see each other's meeting. Stable rooms are allowed (e.g. one personal room per host), but only enable this if you understand the trade-off.
admin-meeting-webhook-help = When an event type's location is set to "Webhook (custom provider)", calrs POSTs the booking payload to this URL on confirmation and expects a JSON body <code>{"{"}"url": "https://..."{"}"}</code> back.
admin-auth-hmac-help = With HMAC, calrs sends <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> over the raw request body.
admin-tls-none-warning = Pick <strong>None</strong> only for a relay on this machine that offers no STARTTLS, or whose certificate is self-signed. Mail, and any credentials, cross the wire in the clear.
admin-smtp-env-error-help = Fix the <code>CALRS_SMTP_*</code> environment variables, or unset them to manage SMTP from the database here.
admin-smtp-env-managed = Managed via <strong>environment variables</strong> (overrides the database). Edit the <code>CALRS_SMTP_*</code> variables to change it, or unset them to manage SMTP from here.
admin-smtp-env-help = Alternatively, configure via environment variables (which override this): <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> or <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Only <code>CALRS_SMTP_HOST</code> and <code>CALRS_SMTP_FROM_EMAIL</code> are required; omit the username and password to relay through a local MTA without authentication.
admin-sms-env-error-help = Fix the <code>CALRS_SMS_*</code> environment variables, or unset them to manage SMS from the database here.
admin-sms-env-managed = Managed via <strong>environment variables</strong> (overrides the database). Edit the <code>CALRS_SMS_*</code> variables to change it, or unset them to manage SMS from here.
admin-sms-env-help = Alternatively, configure via environment variables (which override this): <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>Twilio trial mode is on</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Guests receive Twilio's predefined <code>sms_appointment_reminders</code> template, not the real message, and only numbers verified in your Twilio console will be reached. This is a testing aid for trial accounts. Unset the variable before taking bookings.

admin-show-more =
    { $count ->
        [one] Show { $count } more
       *[other] Show { $count } more
    }

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Pick the protocol your server speaks. EWS targets on-prem Exchange 2019/2016/2013.

admin-sms-going-live = <strong>Before going live:</strong> restrict destination countries in your gateway (Twilio calls this Geo Permissions), keep the account prepaid without auto-recharge, and leave the captcha on. Those three between them bound what an SMS pumping attempt can cost you.

troubleshoot-heading = Troubleshoot availability

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = Name and slug are required.
form-error-team-name-length = Name must be at most 255 characters.
form-error-team-description-length = Description must be at most 5000 characters.
form-error-slug-charset = Slug must contain only lowercase letters, numbers, and dashes.
form-error-slug-reserved = This slug is reserved. Please choose a different one.
form-error-team-slug-taken = A team with this slug already exists.
form-error-title-required = Title is required to generate a slug.
form-error-event-type-slug-taken = An event type with this slug already exists.
form-error-event-type-slug-taken-team = An event type with this slug already exists in this team.
form-error-location-required = Location details are required (e.g. a video call link, phone number, or address).
form-error-not-team-admin = You are not a team admin of this team.
form-error-no-account = No scheduling account found. Please contact an administrator.
form-error-all-fields-required = All fields are required.
form-error-encryption = Encryption error.
form-error-connection-failed = Connection failed: { $error }. Check the URL and credentials, or check "Skip connection test" to save anyway.

# Settings page flash (src/web/mod.rs)

settings-saved = Settings saved.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = Name must be between 1 and 255 characters.
settings-error-username-length = Username must be at least 2 characters.
settings-error-username-taken = This username is already taken.
settings-error-booking-email = Please enter a valid booking email address.
settings-error-save-failed = Failed to save settings.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = Team not found or you are not a team admin.
error-team-not-found = Team not found.
error-event-type-not-found = Event type not found.
error-decrypt-failed = Failed to decrypt stored credentials.
error-source-not-found = Source not found.
error-source-no-password = Source has no stored password.
error-oauth-invalid-state = Invalid state parameter. Please try again.
error-oauth-no-code = No authorization code received.
error-oauth-not-configured = Google OAuth2 not configured.
error-no-scheduling-account = No scheduling account found.
error-private-event-type-not-found = Private event type not found.
error-access-denied = Access denied.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = This slot is no longer available.
error-slot-too-soon = This slot is no longer available (too soon).
error-slot-beyond-horizon = This slot is beyond the booking window.
error-invite-required = This event type requires an invite link.
error-invite-invalid = Invalid invite link.
error-invite-expired = This invite link has expired.
error-invite-used = This invite link has already been used.
error-invalid-date = Invalid date.
error-invalid-time = Invalid time.
error-invalid-date-format = Invalid date format.
error-invalid-time-format = Invalid time format.
error-too-many-bookings = Too many booking attempts. Please try again in a few minutes.
error-too-many-requests = Too many requests. Please try again later.
error-no-members-available = No team members are available for this slot.
error-dynamic-group-public-only = Dynamic group links are only available for public event types.
error-user-not-found = User not found.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Captcha verification failed
bae-title-invalid-booking = Invalid booking details
bae-title-unavailable = Not available right now
bae-title-cannot-approve = Cannot approve this booking
bae-title-invalid-link = Invalid link
bae-title-invalid-or-expired = Invalid or expired link
bae-title-booking-not-found = Booking not found
bae-title-already-approved = Already approved
bae-title-already-declined = Already declined
bae-title-already-cancelled = Already cancelled
bae-title-booking-cancelled = Booking cancelled
bae-title-booking-declined = Booking declined

# Booking action error page: bodies

bae-body-go-back = Please go back and try again.
bae-body-unavailable = The host isn't accepting more bookings for this date. Please pick a different date, or check back later.
bae-body-resource-gone = A required resource is no longer available for this time. Ask the guest to pick another slot.
bae-body-no-claim-token = No claim token provided.
bae-body-claim-invalid = This claim link is no longer valid.
bae-body-booking-gone = This booking no longer exists.
bae-body-decline-link-invalid = This decline link is invalid, has expired, or the booking has already been processed.
bae-body-cancel-link-invalid = This cancellation link is invalid, has expired, or the booking has already been cancelled.
bae-body-cancel-link-invalid-short = This cancellation link is invalid or has expired.
bae-body-reschedule-link-invalid = This reschedule link is invalid, has expired, or the booking has already been processed.
bae-body-approval-link-invalid = This approval link is invalid or has expired.
bae-body-already-approved = This booking has already been approved.
bae-body-already-declined = This booking has already been declined.
bae-body-already-cancelled = This booking has already been cancelled.
bae-body-was-cancelled = This booking was cancelled.
bae-body-declined-by-host = This booking has been declined by the host.

# Booking form validation (src/web/mod.rs)

validate-name-length = Name must be between 1 and 255 characters.
validate-email-length = Email must be between 1 and 255 characters.
validate-email-invalid = Please enter a valid email address.
validate-notes-length = Notes must be 5000 characters or less.
validate-date-too-far = Cannot book more than one year in advance.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = Additional guests are not allowed for this event type.
guests-too-many =
    { $max ->
        [one] You can add at most one additional guest.
       *[other] You can add at most { $max } additional guests.
    }
guests-invalid-email = Invalid additional guest email: { $email }
dynamic-group-min-usernames = Dynamic group links require at least two usernames.
dynamic-group-user-not-found = User "{ $username }" not found.
dynamic-group-user-opted-out = User "{ $username }" has not enabled dynamic group links.

error-slot-unavailable-member = This slot is no longer available ({ $username } has a conflict).
