-- Mark sources that were auto-provisioned by the admin's global EWS config.
-- For managed rows the credentials live in auth_config (not on the row) and
-- impersonate_email tells the SOAP layer which mailbox to act as.

ALTER TABLE caldav_sources ADD COLUMN managed INTEGER NOT NULL DEFAULT 0;
ALTER TABLE caldav_sources ADD COLUMN impersonate_email TEXT;

CREATE INDEX IF NOT EXISTS idx_caldav_sources_managed ON caldav_sources(managed);
