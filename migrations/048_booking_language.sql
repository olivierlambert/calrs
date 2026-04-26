-- Per-booking language tag, captured from the guest's Accept-Language at
-- booking time. Used to render guest-facing emails (confirmation, reminder,
-- cancellation, ...) in the same language they saw the booking page in.
-- NULL means "no signal", which falls back to English at send time.
ALTER TABLE bookings ADD COLUMN language TEXT;
