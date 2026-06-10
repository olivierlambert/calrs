-- Global EWS impersonation: admin-configured central Exchange server with a
-- service account holding the ApplicationImpersonation role. Per-user sources
-- are then auto-provisioned (see 058_caldav_sources_managed.sql) and the SOAP
-- layer injects t:ExchangeImpersonation on every request.

ALTER TABLE auth_config ADD COLUMN ews_global_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE auth_config ADD COLUMN ews_global_url TEXT;
ALTER TABLE auth_config ADD COLUMN ews_service_username TEXT;
ALTER TABLE auth_config ADD COLUMN ews_service_password_enc TEXT;
ALTER TABLE auth_config ADD COLUMN ews_lock_user_sources INTEGER NOT NULL DEFAULT 0;
ALTER TABLE auth_config ADD COLUMN ews_auto_provision INTEGER NOT NULL DEFAULT 0;
