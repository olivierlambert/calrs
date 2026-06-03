-- Add provider_type column to caldav_sources so a single sources table can
-- describe multiple back-end protocols (CalDAV, EWS, ...). Existing rows keep
-- the historical default of 'caldav'.
--
-- Schema reuse note: the `calendars` table keeps its CalDAV-era column names
-- when populated by an EWS source. Specifically:
--   * `calendars.href` holds the EWS folder ItemId (opaque base64-ish blob).
--   * `calendars.ctag` holds the EWS ChangeKey, if any.
-- The semantics are identical (opaque change marker / opaque resource id)
-- and the rest of the codebase treats them that way via the provider trait
-- (`crate::providers::RemoteCalendar`). Renaming the columns would force a
-- table-rebuild migration on existing CalDAV deployments for no behavioural
-- gain, so we accept the misleading names and surface them here.
ALTER TABLE caldav_sources ADD COLUMN provider_type TEXT NOT NULL DEFAULT 'caldav';
CREATE INDEX IF NOT EXISTS idx_caldav_sources_provider_type ON caldav_sources(provider_type);
