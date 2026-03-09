-- calrs auth schema

CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY,
    email           TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    timezone        TEXT NOT NULL DEFAULT 'UTC',
    password_hash   TEXT,                    -- NULL for OIDC-only users
    role            TEXT NOT NULL DEFAULT 'user' CHECK(role IN ('admin', 'user')),
    auth_provider   TEXT NOT NULL DEFAULT 'local' CHECK(auth_provider IN ('local', 'oidc')),
    oidc_subject    TEXT,                    -- OIDC 'sub' claim
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    id              TEXT PRIMARY KEY,        -- random token
    user_id         TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at      TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);

CREATE TABLE IF NOT EXISTS auth_config (
    id                      TEXT PRIMARY KEY DEFAULT 'singleton',
    registration_enabled    INTEGER NOT NULL DEFAULT 1,
    allowed_email_domains   TEXT,            -- comma-separated, NULL = any domain
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Ensure there's always exactly one auth_config row
INSERT OR IGNORE INTO auth_config (id) VALUES ('singleton');

-- Link accounts (scheduling profiles) to users
-- Existing accounts won't have this set yet; migration path handled in code
ALTER TABLE accounts ADD COLUMN user_id TEXT REFERENCES users(id) ON DELETE SET NULL;

CREATE TABLE IF NOT EXISTS groups (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    source      TEXT NOT NULL DEFAULT 'local' CHECK(source IN ('local', 'oidc')),
    oidc_id     TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS user_groups (
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    group_id    TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, group_id)
);
