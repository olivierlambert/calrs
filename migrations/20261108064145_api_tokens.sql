-- REST API (#169): personal access tokens, idempotency keys, and who may
-- create tokens.

-- A token a user created for a script. Only the SHA-256 hash of the secret is
-- kept (oauth::hash); the secret is shown once, when it is created.
CREATE TABLE api_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    -- The first characters of the secret after its prefix, shown in the list
    -- so the user can tell two tokens apart.
    hint TEXT NOT NULL,
    -- Space-separated API scopes (oauth::API_SCOPES).
    scope TEXT NOT NULL,
    -- The creating session had passed local MFA (#213): the same policy as
    -- an OAuth grant (oauth::GRANT_MFA_POLICY) applies to the token.
    mfa_verified INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    -- Unix time after which the token stops working; NULL = never.
    expires_at INTEGER,
    last_used_at INTEGER
);
CREATE INDEX idx_api_tokens_user ON api_tokens(user_id);

-- Idempotency-Key of POST /api/v1/bookings, kept 24 hours. Scoped per user,
-- so two users cannot collide or read each other's answers. `response` is
-- NULL while the first request is still running.
CREATE TABLE api_idempotency (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    status INTEGER,
    response TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, key)
);
CREATE INDEX idx_api_idempotency_created ON api_idempotency(created_at);

-- Off by default: only admins may create personal access tokens. OAuth
-- clients, which every grant approves interactively, are not affected.
ALTER TABLE auth_config ADD COLUMN api_tokens_allow_all_users INTEGER NOT NULL DEFAULT 0;
