-- #324: per-calendar choice of what a source syncs. A calendar with
-- sync_enabled = 0 is still listed (discovery keeps it up to date) but its
-- events are never fetched and it never blocks availability. Existing and
-- newly discovered calendars default to synced, as before.
ALTER TABLE calendars ADD COLUMN sync_enabled INTEGER NOT NULL DEFAULT 1;
