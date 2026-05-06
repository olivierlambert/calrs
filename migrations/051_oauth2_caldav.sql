-- OAuth2 authentication support for CalDAV sources (e.g. Google Calendar)
ALTER TABLE caldav_sources ADD COLUMN auth_type TEXT NOT NULL DEFAULT 'basic';
ALTER TABLE caldav_sources ADD COLUMN oauth2_provider TEXT;
ALTER TABLE caldav_sources ADD COLUMN access_token_enc TEXT;
ALTER TABLE caldav_sources ADD COLUMN refresh_token_enc TEXT;
ALTER TABLE caldav_sources ADD COLUMN token_expires_at TEXT;

-- Google OAuth2 client credentials (admin-configured, instance-wide)
ALTER TABLE auth_config ADD COLUMN google_oauth2_client_id TEXT;
ALTER TABLE auth_config ADD COLUMN google_oauth2_client_secret TEXT;
