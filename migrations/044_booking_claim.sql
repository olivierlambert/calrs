ALTER TABLE bookings ADD COLUMN claimed_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE bookings ADD COLUMN claimed_at TEXT;

CREATE TABLE IF NOT EXISTS booking_claim_tokens (
    id TEXT PRIMARY KEY,
    booking_id TEXT NOT NULL REFERENCES bookings(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    used_at TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_booking_claim_tokens_token ON booking_claim_tokens(token);
CREATE INDEX IF NOT EXISTS idx_booking_claim_tokens_booking ON booking_claim_tokens(booking_id);
