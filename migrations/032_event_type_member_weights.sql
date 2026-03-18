-- Per-event-type member priority for round-robin assignment
CREATE TABLE IF NOT EXISTS event_type_member_weights (
    event_type_id TEXT NOT NULL REFERENCES event_types(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    weight INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (event_type_id, user_id)
);
