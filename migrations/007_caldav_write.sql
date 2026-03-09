-- Track CalDAV write-back: which calendar a booking was pushed to
ALTER TABLE bookings ADD COLUMN caldav_calendar_href TEXT;

-- Let users pick a default calendar for booking write-back (per source)
ALTER TABLE caldav_sources ADD COLUMN write_calendar_href TEXT;
