-- Follow-up to #324: what a source does with a calendar it lists for the
-- first time. 1 (the default, as before): synced. 0: listed, but not synced
-- until its owner turns it on. A CalDAV source added from the dashboard is
-- created with 0 and set back to 1 once its calendars are listed, so no sync
-- running in between can fetch a calendar before the owner picked it.
ALTER TABLE caldav_sources ADD COLUMN sync_new_calendars INTEGER NOT NULL DEFAULT 1;
