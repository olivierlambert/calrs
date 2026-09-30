ALTER TABLE auth_config ADD COLUMN mfa_required INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN mfa_verified INTEGER NOT NULL DEFAULT 0;

-- Secrets stay separate from the User model used in templates and JSON.
CREATE TABLE user_mfa (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret_enc TEXT NOT NULL,
    last_step INTEGER NOT NULL
);
CREATE TABLE mfa_recovery_codes (
    user_id TEXT NOT NULL REFERENCES user_mfa(user_id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL,
    PRIMARY KEY (user_id, code_hash)
);
CREATE TABLE mfa_challenges (
    token_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK (purpose IN ('login', 'enroll')),
    secret_enc TEXT,
    expires_at INTEGER NOT NULL
);
CREATE INDEX mfa_challenges_user ON mfa_challenges(user_id);
CREATE TABLE mfa_attempts (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    window_start INTEGER NOT NULL,
    attempts INTEGER NOT NULL
);
