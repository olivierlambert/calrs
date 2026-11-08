-- writeback_verified_at
-- When the host's calendar copy of a booking was last read back after a
-- write (#162), UTC "YYYY-MM-DD HH:MM:SS". The Graph orphan sweep cancels a
-- booking with no cached event only when this predates the snapshot.
ALTER TABLE bookings ADD COLUMN writeback_verified_at TEXT;
