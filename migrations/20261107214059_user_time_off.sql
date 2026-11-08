-- user_time_off: periods a user takes no bookings, across every event type
-- they host (#46). `start_at` and `end_at` are UTC instants
-- ('YYYY-MM-DDTHH:MM:SSZ', end exclusive), so the busy query compares them
-- as strings. `timezone` is the zone the period was entered in, used only
-- to show it back as it was typed. `label` is private to the user.
CREATE TABLE user_time_off (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    start_at TEXT NOT NULL,
    end_at TEXT NOT NULL,
    timezone TEXT NOT NULL,
    label TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX idx_user_time_off_user_end ON user_time_off(user_id, end_at);
