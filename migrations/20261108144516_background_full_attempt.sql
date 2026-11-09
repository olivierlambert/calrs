-- background_full_attempt
-- When the background sync last started a forced full resync of the source
-- (#388). A source due its daily full resync is synced ahead of the others,
-- but at most once every 6 hours: a sync that succeeds without completing a full
-- snapshot (a Microsoft source still missing an item, a source with no
-- synced calendar) leaves `last_full_sync` old, and would otherwise jump the
-- queue on every tick.
ALTER TABLE caldav_sources ADD COLUMN last_full_attempt_at TEXT;
