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
    -- Renewed every 30 seconds while the first request runs; a key whose
    -- lease is two minutes old was abandoned (the process stopped).
    lease_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, key)
);
CREATE INDEX idx_api_idempotency_created ON api_idempotency(created_at);

-- Bookings created through the API, for the per-user daily cap (each one
-- sends emails from the instance). Kept a day.
CREATE TABLE api_booking_log (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    booking_id TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_api_booking_log_user ON api_booking_log(user_id, created_at);

-- Off by default: only admins may use the REST API, with a personal access
-- token or an OAuth grant for the API resource (oauth::api_allowed). MCP
-- grants are not affected.
ALTER TABLE auth_config ADD COLUMN api_tokens_allow_all_users INTEGER NOT NULL DEFAULT 0;

-- When the host last emailed the guest a reschedule request, so a pending
-- request is resent (same link) at most once an hour.
ALTER TABLE bookings ADD COLUMN reschedule_requested_at INTEGER;
