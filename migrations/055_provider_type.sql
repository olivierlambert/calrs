-- Add provider_type column to caldav_sources so a single sources table can
-- describe multiple back-end protocols (CalDAV, EWS, ...). Existing rows keep
-- the historical default of 'caldav'.
ALTER TABLE caldav_sources ADD COLUMN provider_type TEXT NOT NULL DEFAULT 'caldav';
CREATE INDEX IF NOT EXISTS idx_caldav_sources_provider_type ON caldav_sources(provider_type);
