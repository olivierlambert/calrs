-- Native Microsoft Graph, independent of sign-in/OIDC identity.
ALTER TABLE auth_config ADD COLUMN microsoft_oauth2_client_id TEXT;
ALTER TABLE auth_config ADD COLUMN microsoft_oauth2_client_secret TEXT;
-- Deliberately supports both work/school and personal Microsoft accounts.
ALTER TABLE auth_config ADD COLUMN microsoft_oauth2_tenant TEXT NOT NULL DEFAULT 'common';
ALTER TABLE caldav_sources ADD COLUMN microsoft_user_id TEXT;
CREATE UNIQUE INDEX idx_microsoft_source_identity ON caldav_sources(account_id, microsoft_user_id)
    WHERE provider_type = 'microsoft';
CREATE TABLE microsoft_oauth_states (
    state_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    verifier_enc TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    client_id TEXT NOT NULL,
    tenant TEXT NOT NULL,
    reconnect_source_id TEXT REFERENCES caldav_sources(id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);
CREATE INDEX idx_microsoft_oauth_state_expiry ON microsoft_oauth_states(expires_at);
