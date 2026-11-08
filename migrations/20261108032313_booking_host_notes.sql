-- Host notes on a booking (#173): free text the booking's hosts write after
-- the booking exists, kept apart from the guest's own `notes`. Shown on host
-- surfaces only (the dashboard, host emails), never to the guest, in an
-- invite, an ICS file, a calendar write-back, an SMS or a webhook. NULL means
-- no note. Edited in place, so a reschedule (which updates the row) keeps it,
-- and a cancellation (a status change) does too.
ALTER TABLE bookings ADD COLUMN host_notes TEXT;
-- Bumped by every save. A save names the version it was loaded with and is
-- refused when it changed, so a save made from a stale page does not
-- silently overwrite what another host wrote meanwhile.
ALTER TABLE bookings ADD COLUMN host_notes_version INTEGER NOT NULL DEFAULT 0;
-- When the note was last saved (UTC, `YYYY-MM-DD HH:MM:SS`) and by whom, so
-- the hosts of a shared booking can tell who wrote it. A save made by an
-- admin impersonating a host records the admin in `updated_by` and the host
-- in `updated_as` (NULL otherwise).
ALTER TABLE bookings ADD COLUMN host_notes_updated_at TEXT;
ALTER TABLE bookings ADD COLUMN host_notes_updated_by TEXT REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE bookings ADD COLUMN host_notes_updated_as TEXT REFERENCES users(id) ON DELETE SET NULL;
