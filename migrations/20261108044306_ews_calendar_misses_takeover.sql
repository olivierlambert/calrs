-- ews_calendar_misses_takeover
--
-- An EWS calendar the server stops listing is only dropped after it is
-- missing from two successful listings in a row: one odd answer (a proxy
-- error page, a folder briefly unreadable) must not take the calendar, its
-- cached events and the event types' selections of it. Reset to 0 whenever
-- the folder is listed again.
ALTER TABLE calendars ADD COLUMN ews_missing_count INTEGER NOT NULL DEFAULT 0;

-- When an owner the identity provider verified takes a mailbox over from an
-- account that was only admin-confirmed, the admin's status table says from
-- whom (that account's address at the time) and when.
ALTER TABLE caldav_sources ADD COLUMN taken_over_from TEXT;
ALTER TABLE caldav_sources ADD COLUMN taken_over_at TEXT;
