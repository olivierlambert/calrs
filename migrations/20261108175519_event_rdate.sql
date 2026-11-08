-- RDATE (#376): a series' extra dates are read from its iCal text, and a
-- row with them is expanded like a recurring one, even without an RRULE.
-- Set by sync for the series row itself (never for a moved occurrence).
ALTER TABLE events ADD COLUMN rdate INTEGER NOT NULL DEFAULT 0;
ALTER TABLE resource_events ADD COLUMN rdate INTEGER NOT NULL DEFAULT 0;

-- Rows cached before: a series row whose iCal text names an RDATE.
UPDATE events SET rdate = 1 WHERE recurrence_id IS NULL AND raw_ical LIKE '%RDATE%';
UPDATE resource_events SET rdate = 1 WHERE recurrence_id IS NULL AND raw_ical LIKE '%RDATE%';
