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

# Slot picker (templates/slots.html)

slots-location-video = Video call
slots-location-phone = Phone call

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
book-notes-label = Notes
book-notes-optional = (optional)
book-notes-placeholder = Anything you'd like to discuss?
book-additional-guests-label = Additional guests
book-additional-guests-hint = (optional, up to { $max })
book-add-guest-btn = + Add guest email
book-guest-email-placeholder = colleague@example.com
book-confirm-button = Confirm booking

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
