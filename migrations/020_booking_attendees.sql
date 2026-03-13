-- Allow guests to add additional attendees to bookings.
-- max_additional_guests on event_types: 0 = disabled (default).
ALTER TABLE event_types ADD COLUMN max_additional_guests INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS booking_attendees (
    id TEXT PRIMARY KEY,
    booking_id TEXT NOT NULL REFERENCES bookings(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
