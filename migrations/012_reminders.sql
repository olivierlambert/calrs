ALTER TABLE event_types ADD COLUMN reminder_minutes INTEGER;
ALTER TABLE bookings ADD COLUMN reminder_sent_at TEXT;
