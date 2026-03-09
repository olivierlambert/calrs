-- OIDC configuration columns on auth_config
ALTER TABLE auth_config ADD COLUMN oidc_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE auth_config ADD COLUMN oidc_issuer_url TEXT;
ALTER TABLE auth_config ADD COLUMN oidc_client_id TEXT;
ALTER TABLE auth_config ADD COLUMN oidc_client_secret TEXT;
ALTER TABLE auth_config ADD COLUMN oidc_auto_register INTEGER NOT NULL DEFAULT 1;
