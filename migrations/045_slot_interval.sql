-- Configurable slot interval independent from event duration.
-- NULL (default) means slot interval equals duration_min (prior behavior).
ALTER TABLE event_types ADD COLUMN slot_interval_min INTEGER;
