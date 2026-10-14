-- API-fetched status events have their own snapshot lifecycle, independent of
-- CalDAV ctags, sync tokens and orphan reconciliation.
ALTER TABLE events ADD COLUMN google_out_of_office INTEGER NOT NULL DEFAULT 0;
