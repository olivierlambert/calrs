-- #324: per-calendar choice of what a source syncs. A calendar with
-- sync_enabled = 0 is still listed (discovery keeps it up to date) but its
-- events are never fetched and it never blocks availability. Existing and
-- newly discovered calendars default to synced, as before.
ALTER TABLE calendars ADD COLUMN sync_enabled INTEGER NOT NULL DEFAULT 1;
-- Bumped by every change of sync_enabled. A sync only stores the ctag and
-- sync token it fetched when the generation it started with is unchanged,
-- so a calendar turned off and on again during a sync is fully refetched.
ALTER TABLE calendars ADD COLUMN sync_generation INTEGER NOT NULL DEFAULT 0;
