-- Lead capture (iClosed-style): record what guests type into a booking form
-- before they submit. Off by default at every level — both an admin global
-- switch and a per-event-type toggle must be on for capture to happen, and
-- the host must declare they have informed bookers (RGPD).

CREATE TABLE IF NOT EXISTS partial_bookings (
    id              TEXT PRIMARY KEY,
    event_type_id   TEXT NOT NULL REFERENCES event_types(id) ON DELETE CASCADE,
    -- Resolved at insert-time so the dashboard "leads" page can scope per-host
    -- without joining through event_types every time. NULL is allowed because
    -- legacy event types may have no created_by_user_id.
    host_user_id    TEXT REFERENCES users(id) ON DELETE SET NULL,
    -- Stable id sent by the browser (sessionStorage). Treated as opaque.
    -- UNIQUE so the upsert path is straightforward.
    lead_id         TEXT NOT NULL UNIQUE,
    -- Captured fields. All optional — capture happens incrementally as the
    -- guest types, so a row may have only a partial email address.
    name            TEXT,
    email           TEXT,
    phone           TEXT,
    notes           TEXT,
    -- Audit metadata
    ip              TEXT,
    user_agent      TEXT,
    -- The slot the guest is targeting (mirrors the hidden fields in the
    -- booking form). Useful for dashboard context and follow-up messages.
    target_date     TEXT,
    target_time     TEXT,
    target_tz       TEXT,
    -- Set when the guest finishes the booking. NULL = still partial.
    completed_at    TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_partial_bookings_event_type
    ON partial_bookings(event_type_id);
CREATE INDEX IF NOT EXISTS idx_partial_bookings_host
    ON partial_bookings(host_user_id);
CREATE INDEX IF NOT EXISTS idx_partial_bookings_completed
    ON partial_bookings(completed_at);
CREATE INDEX IF NOT EXISTS idx_partial_bookings_updated
    ON partial_bookings(updated_at);

-- Per-event-type opt-in toggle. Off by default; the host must turn it on
-- per event type, and they have to acknowledge they've informed bookers.
ALTER TABLE event_types ADD COLUMN lead_capture INTEGER NOT NULL DEFAULT 0;

-- Global feature switch + retention window. Both live on auth_config so the
-- admin panel can manage them. Default: feature off, 30-day retention so
-- captured rows are auto-purged.
ALTER TABLE auth_config ADD COLUMN lead_capture_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE auth_config ADD COLUMN lead_retention_days INTEGER NOT NULL DEFAULT 30;
