-- Add is_private flag to event_types
ALTER TABLE event_types ADD COLUMN is_private INTEGER NOT NULL DEFAULT 0;

-- Booking invites table
CREATE TABLE IF NOT EXISTS booking_invites (
    id TEXT PRIMARY KEY,
    event_type_id TEXT NOT NULL REFERENCES event_types(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    guest_name TEXT NOT NULL,
    guest_email TEXT NOT NULL,
    message TEXT,
    expires_at TEXT,
    max_uses INTEGER NOT NULL DEFAULT 1,
    used_count INTEGER NOT NULL DEFAULT 0,
    created_by_user_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_booking_invites_token ON booking_invites(token);
CREATE INDEX IF NOT EXISTS idx_booking_invites_event_type ON booking_invites(event_type_id);
