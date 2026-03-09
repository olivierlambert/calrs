-- Change unique constraint from uid alone to (uid, recurrence_id).
-- A recurring event and its modified instances share the same UID but have
-- different RECURRENCE-ID values.  The parent has NULL recurrence_id.

-- SQLite cannot ALTER a UNIQUE constraint, so we recreate the table.
CREATE TABLE events_new (
    id              TEXT PRIMARY KEY,
    calendar_id     TEXT NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
    uid             TEXT NOT NULL,
    etag            TEXT,
    summary         TEXT,
    description     TEXT,
    location        TEXT,
    start_at        TEXT NOT NULL,
    end_at          TEXT NOT NULL,
    all_day         INTEGER NOT NULL DEFAULT 0,
    timezone        TEXT,
    rrule           TEXT,
    status          TEXT DEFAULT 'confirmed',
    raw_ical        TEXT,
    synced_at       TEXT NOT NULL DEFAULT (datetime('now')),
    recurrence_id   TEXT
);

INSERT INTO events_new (id, calendar_id, uid, etag, summary, description, location, start_at, end_at, all_day, timezone, rrule, status, raw_ical, synced_at, recurrence_id)
    SELECT id, calendar_id, uid, etag, summary, description, location, start_at, end_at, all_day, timezone, rrule, status, raw_ical, synced_at, recurrence_id FROM events;

DROP TABLE events;
ALTER TABLE events_new RENAME TO events;

CREATE UNIQUE INDEX idx_events_uid_recurrence ON events(uid, COALESCE(recurrence_id, ''));
CREATE INDEX IF NOT EXISTS idx_events_calendar ON events(calendar_id);
CREATE INDEX IF NOT EXISTS idx_events_start ON events(start_at);
