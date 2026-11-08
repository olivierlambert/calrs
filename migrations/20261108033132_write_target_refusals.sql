-- write_target_refusals (#47)
--
-- When a chosen destination calendar definitively refuses a booking (HTTP
-- 403, 404 or 405: deleted on the server, or read-only), the booking is
-- written to the source setting instead, and the refusal is kept here so
-- the sources page can tell the host. The next booking tries the chosen
-- calendar again; a write that lands there clears both columns.
ALTER TABLE event_type_write_targets ADD COLUMN last_refused_at TEXT;
ALTER TABLE event_type_write_targets ADD COLUMN last_refused_error TEXT;

-- The source a booking's caldav_calendar_href is on. Two servers can use
-- the same calendar path, and the href alone then names the wrong copy for
-- the orphan sweep and for deletes. NULL on bookings written before: they
-- keep the previous matching by href.
ALTER TABLE bookings ADD COLUMN caldav_source_id TEXT REFERENCES caldav_sources(id) ON DELETE SET NULL;

-- Background retries of a failed delete of a recorded copy, for bookings
-- that ended: how many were tried, and when the next one is due.
ALTER TABLE booking_write_copies ADD COLUMN delete_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE booking_write_copies ADD COLUMN next_delete_at TEXT;
