-- Persist the most recent sync error per source so the admin can see, at a
-- glance, which managed Exchange users synced and which failed (e.g. an admin
-- account with no Exchange mailbox). NULL = the last attempt succeeded.
ALTER TABLE caldav_sources ADD COLUMN last_sync_error TEXT;
