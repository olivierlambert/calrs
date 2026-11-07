-- Host notes on a booking (#173): free text the booking's hosts write after
-- the booking exists, kept apart from the guest's own `notes`. Shown on host
-- surfaces only (the dashboard, host emails), never to the guest, in an
-- invite, an ICS file, a calendar write-back, an SMS or a webhook. NULL means
-- no note. Edited in place, so a reschedule (which updates the row) keeps it,
-- and a cancellation (a status change) does too.
ALTER TABLE bookings ADD COLUMN host_notes TEXT;
-- When the note was last saved (UTC, `YYYY-MM-DD HH:MM:SS.SSS`) and by whom, so
-- the hosts of a shared booking can tell who wrote it, and a save made from
-- a stale page does not silently overwrite a newer one.
ALTER TABLE bookings ADD COLUMN host_notes_updated_at TEXT;
ALTER TABLE bookings ADD COLUMN host_notes_updated_by TEXT REFERENCES users(id) ON DELETE SET NULL;
