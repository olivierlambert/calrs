-- Per-resource team allowlist: team admins of an allowlisted team may attach
-- the resource to that team's event types. An empty allowlist keeps the
-- resource manageable by global admins only (previous behavior).
CREATE TABLE resource_teams (
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    team_id TEXT NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    PRIMARY KEY (resource_id, team_id)
);
