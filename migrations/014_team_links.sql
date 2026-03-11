-- Ad-hoc team links: shareable booking links across multiple users
CREATE TABLE IF NOT EXISTS team_links (
    id TEXT PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    duration_min INTEGER NOT NULL DEFAULT 30,
    buffer_before INTEGER NOT NULL DEFAULT 0,
    buffer_after INTEGER NOT NULL DEFAULT 0,
    min_notice_min INTEGER NOT NULL DEFAULT 60,
    availability_start TEXT NOT NULL DEFAULT '09:00',
    availability_end TEXT NOT NULL DEFAULT '17:00',
    availability_days TEXT NOT NULL DEFAULT '1,2,3,4,5',
    created_by_user_id TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS team_link_members (
    id TEXT PRIMARY KEY,
    team_link_id TEXT NOT NULL REFERENCES team_links(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id),
    UNIQUE(team_link_id, user_id)
);

CREATE TABLE IF NOT EXISTS team_link_bookings (
    id TEXT PRIMARY KEY,
    team_link_id TEXT NOT NULL REFERENCES team_links(id) ON DELETE CASCADE,
    uid TEXT NOT NULL,
    guest_name TEXT NOT NULL,
    guest_email TEXT NOT NULL,
    guest_timezone TEXT,
    notes TEXT,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'confirmed',
    cancel_token TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
