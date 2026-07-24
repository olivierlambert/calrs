-- Shared bookable resources (demo lab, meeting room, …) backed by a calendar
-- feed. Instance-level entities: not owned by a user account.
CREATE TABLE resources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    -- ICS publish URL, read-only, used for availability (no credentials).
    feed_url TEXT NOT NULL,
    -- Optional CalDAV collection URL for write-back (reservation).
    caldav_url TEXT,
    -- Optional service-account credentials for write-back (encrypted).
    caldav_username TEXT,
    caldav_password TEXT,
    last_synced_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Cached events from the resource feed (same role as `events` for sources).
CREATE TABLE resource_events (
    id TEXT PRIMARY KEY,
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    uid TEXT NOT NULL,
    recurrence_id TEXT,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL,
    all_day INTEGER NOT NULL DEFAULT 0,
    timezone TEXT,
    rrule TEXT,
    raw_ical TEXT,
    status TEXT,
    transp TEXT,
    summary TEXT
);
CREATE UNIQUE INDEX idx_resource_events_uid
    ON resource_events(resource_id, uid, COALESCE(recurrence_id, ''));

-- Which resources an event type needs.
CREATE TABLE event_type_resources (
    event_type_id TEXT NOT NULL REFERENCES event_types(id) ON DELETE CASCADE,
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    PRIMARY KEY (event_type_id, resource_id)
);

-- 'all' = every attached resource must be free (default),
-- 'round_robin' = one free resource is picked and assigned.
ALTER TABLE event_types ADD COLUMN resource_scheduling_mode TEXT NOT NULL DEFAULT 'all';

-- Round-robin resource assignment. Write-back tracking needs no extra
-- columns: reservations are PUT under the booking's own uid, so release
-- deletes by uid with whatever write credentials are currently valid.
ALTER TABLE bookings ADD COLUMN assigned_resource_id TEXT;

-- Member opt-in: allow calrs to use my CalDAV credentials to write
-- reservations into resource calendars I have write access to.
ALTER TABLE users ADD COLUMN lend_resource_write INTEGER NOT NULL DEFAULT 0;
