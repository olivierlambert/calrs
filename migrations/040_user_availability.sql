-- Per-user default availability rules (used by dynamic group links)
CREATE TABLE IF NOT EXISTS user_availability_rules (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    day_of_week INTEGER NOT NULL CHECK(day_of_week BETWEEN 0 AND 6),
    start_time  TEXT NOT NULL,
    end_time    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_user_availability_rules_user ON user_availability_rules(user_id);
