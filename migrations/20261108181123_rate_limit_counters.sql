-- rate_limit_counters
-- Persistent per-key counters for the booking rate limiter (#85, FINDING-10),
-- so a restart no longer hands every address a fresh budget. One row per
-- (bucket, key): `window_start` is when the current window opened (unix
-- seconds), `expires_at` when it closes, which is what the hourly prune reads.
CREATE TABLE IF NOT EXISTS rate_limit_counters (
    bucket TEXT NOT NULL,
    key TEXT NOT NULL,
    window_start INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY (bucket, key)
);

CREATE INDEX IF NOT EXISTS idx_rate_limit_counters_expires
    ON rate_limit_counters(expires_at);
