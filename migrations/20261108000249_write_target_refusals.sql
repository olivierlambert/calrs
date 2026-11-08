-- write_target_refusals (#47)
--
-- When a chosen destination calendar definitively refuses a booking (HTTP
-- 403, 404 or 405: deleted on the server, or read-only), the booking is
-- written to the source setting instead, and the refusal is kept here so
-- the sources page can tell the host. The next booking tries the chosen
-- calendar again; a write that lands there clears both columns.
ALTER TABLE event_type_write_targets ADD COLUMN last_refused_at TEXT;
ALTER TABLE event_type_write_targets ADD COLUMN last_refused_error TEXT;
