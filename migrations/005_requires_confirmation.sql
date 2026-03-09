-- Add requires_confirmation flag to event_types
ALTER TABLE event_types ADD COLUMN requires_confirmation INTEGER NOT NULL DEFAULT 0;
