-- Per-event-type rolling booking horizon: how many days into the future a
-- guest may book. NULL means unlimited (the existing behaviour).
ALTER TABLE event_types ADD COLUMN booking_horizon_days INTEGER;
