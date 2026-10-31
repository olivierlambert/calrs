-- Per-event-type bookable date range: the calendar days (in the event type's
-- timezone, both bounds inclusive) on which an appointment may take place.
-- NULL on either side means no bound there (the existing behaviour).
ALTER TABLE event_types ADD COLUMN bookable_from TEXT;   -- YYYY-MM-DD
ALTER TABLE event_types ADD COLUMN bookable_until TEXT;  -- YYYY-MM-DD
