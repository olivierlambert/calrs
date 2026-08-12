-- SMS notifications via Twilio (optional, opt-in per event type).
--
-- Mirrors the smtp_config pattern: a system-wide singleton row, auth token
-- encrypted at rest with AES-256-GCM (see src/crypto.rs), editable from the
-- admin panel or CALRS_TWILIO_* environment variables.

-- Guests can optionally leave a phone number on the booking form. It is only
-- collected/shown when the event type has SMS notifications enabled.
ALTER TABLE bookings ADD COLUMN phone_number TEXT;

-- Per-event-type opt-in switch (default off, so existing event types keep
-- working exactly as before with no SMS involved).
ALTER TABLE event_types ADD COLUMN sms_notifications_enabled INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS twilio_config (
    id                TEXT PRIMARY KEY,
    account_sid       TEXT NOT NULL,
    auth_token_enc     TEXT,
    from_number       TEXT NOT NULL,
    enabled           INTEGER NOT NULL DEFAULT 1,
    created_at        TEXT NOT NULL DEFAULT (datetime('now'))
);
