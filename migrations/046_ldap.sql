-- LDAP authentication support
--
-- Widens users.auth_provider and groups.source CHECK constraints to include
-- 'ldap', adds users.ldap_dn for identity linking, and adds LDAP config columns
-- to auth_config. SQLite cannot alter CHECK constraints in place, so the
-- users and groups tables are rebuilt with the 12-step pattern from
-- https://sqlite.org/lang_altertable.html §7.
--
-- Foreign keys pointing at these tables (sessions.user_id, accounts.user_id,
-- user_groups, team_members, event_types.group_id, team_groups, etc.) refer
-- by table name, so they re-resolve after the RENAME. PRAGMA foreign_keys=OFF
-- is required during the rebuild.

PRAGMA foreign_keys = OFF;

-- --- Rebuild users ---

CREATE TABLE users_new (
    id                  TEXT PRIMARY KEY,
    email               TEXT NOT NULL UNIQUE,
    name                TEXT NOT NULL,
    timezone            TEXT NOT NULL DEFAULT 'UTC',
    password_hash       TEXT,
    role                TEXT NOT NULL DEFAULT 'user' CHECK(role IN ('admin', 'user')),
    auth_provider       TEXT NOT NULL DEFAULT 'local' CHECK(auth_provider IN ('local', 'oidc', 'ldap')),
    oidc_subject        TEXT,
    ldap_dn             TEXT,
    enabled             INTEGER NOT NULL DEFAULT 1,
    username            TEXT,
    booking_email       TEXT,
    title               TEXT,
    bio                 TEXT,
    avatar_path         TEXT,
    allow_dynamic_group INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO users_new (
    id, email, name, timezone, password_hash, role, auth_provider, oidc_subject,
    enabled, username, booking_email, title, bio, avatar_path,
    allow_dynamic_group, created_at, updated_at
)
SELECT
    id, email, name, timezone, password_hash, role, auth_provider, oidc_subject,
    enabled, username, booking_email, title, bio, avatar_path,
    allow_dynamic_group, created_at, updated_at
FROM users;

DROP TABLE users;
ALTER TABLE users_new RENAME TO users;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username ON users(username);

-- --- Rebuild groups ---

CREATE TABLE groups_new (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    source      TEXT NOT NULL DEFAULT 'local' CHECK(source IN ('local', 'oidc', 'ldap')),
    oidc_id     TEXT,
    slug        TEXT,
    description TEXT,
    avatar_path TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO groups_new (id, name, source, oidc_id, slug, description, avatar_path, created_at)
SELECT id, name, source, oidc_id, slug, description, avatar_path, created_at FROM groups;

DROP TABLE groups;
ALTER TABLE groups_new RENAME TO groups;

CREATE UNIQUE INDEX IF NOT EXISTS idx_groups_slug ON groups(slug);

-- --- LDAP config on auth_config ---
-- ldap_bind_password is AES-256-GCM encrypted via src/crypto.rs.
-- ldap_tls_mode: 'ldaps' (implicit TLS on port 636), 'starttls' (StartTLS on
-- 389), or 'plain' (no encryption, opt-in only — will warn at runtime).
-- ldap_user_filter: must contain '{username}' as a placeholder that is
-- RFC 4515-escaped at query time (see auth::ldap::escape_filter).

ALTER TABLE auth_config ADD COLUMN ldap_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE auth_config ADD COLUMN ldap_server_url TEXT;
ALTER TABLE auth_config ADD COLUMN ldap_tls_mode TEXT NOT NULL DEFAULT 'starttls' CHECK(ldap_tls_mode IN ('ldaps', 'starttls', 'plain'));
ALTER TABLE auth_config ADD COLUMN ldap_bind_dn TEXT;
ALTER TABLE auth_config ADD COLUMN ldap_bind_password TEXT;
ALTER TABLE auth_config ADD COLUMN ldap_user_search_base TEXT;
ALTER TABLE auth_config ADD COLUMN ldap_user_filter TEXT NOT NULL DEFAULT '(uid={username})';
ALTER TABLE auth_config ADD COLUMN ldap_email_attr TEXT NOT NULL DEFAULT 'mail';
ALTER TABLE auth_config ADD COLUMN ldap_name_attr TEXT NOT NULL DEFAULT 'cn';
ALTER TABLE auth_config ADD COLUMN ldap_groups_attr TEXT;
ALTER TABLE auth_config ADD COLUMN ldap_auto_register INTEGER NOT NULL DEFAULT 1;

PRAGMA foreign_keys = ON;
