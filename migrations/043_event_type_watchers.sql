CREATE TABLE IF NOT EXISTS event_type_watchers (
    event_type_id TEXT NOT NULL REFERENCES event_types(id) ON DELETE CASCADE,
    team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    PRIMARY KEY (event_type_id, team_id)
);
