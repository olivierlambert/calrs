-- Favorites refer to live meeting types, never to copied URLs or invite tokens.
CREATE TABLE event_type_favorites (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_type_id TEXT NOT NULL REFERENCES event_types(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (user_id, event_type_id)
);

CREATE INDEX idx_event_type_favorites_event ON event_type_favorites(event_type_id);
