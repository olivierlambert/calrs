-- Failure indicator for resource feed syncs. last_synced_at doubles as the
-- backoff stamp (set even on failure), so a separate column is needed to
-- tell the admin the last attempt failed.
ALTER TABLE resources ADD COLUMN last_sync_error TEXT;
