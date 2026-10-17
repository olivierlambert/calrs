-- Public OAuth clients register themselves; no operator-managed credentials.
CREATE TABLE oauth_clients (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    redirect_uris TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE oauth_requests (
    id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth_clients(id) ON DELETE CASCADE,
    parameters TEXT NOT NULL,
    user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);
CREATE TABLE oauth_grants (
    id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES oauth_clients(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    resource TEXT NOT NULL,
    scope TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE oauth_credentials (
    hash TEXT PRIMARY KEY,
    grant_id TEXT NOT NULL REFERENCES oauth_grants(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('code', 'access', 'refresh')),
    expires_at INTEGER NOT NULL,
    used INTEGER NOT NULL DEFAULT 0,
    redirect_uri TEXT,
    challenge TEXT
);
CREATE INDEX idx_oauth_credentials_grant ON oauth_credentials(grant_id);
CREATE INDEX idx_oauth_grants_expiry ON oauth_grants(expires_at);
CREATE INDEX idx_oauth_requests_expiry ON oauth_requests(expires_at);
