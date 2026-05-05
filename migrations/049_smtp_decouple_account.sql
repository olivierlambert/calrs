-- Decouple smtp_config from accounts. SMTP is used as a system-wide singleton
-- everywhere in the app (load_smtp_config does SELECT ... LIMIT 1, ignoring
-- account_id), but the schema bound the row to an account with ON DELETE
-- CASCADE. That meant deleting the user whose account happened to own the row
-- silently wiped the instance-wide SMTP config (issue #67).
--
-- SQLite cannot drop a column or FK constraint, so we recreate the table.

CREATE TABLE smtp_config_new (
    id              TEXT PRIMARY KEY,
    host            TEXT NOT NULL,
    port            INTEGER NOT NULL DEFAULT 587,
    username        TEXT NOT NULL,
    password_enc    TEXT,
    from_email      TEXT NOT NULL,
    from_name       TEXT,
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO smtp_config_new (id, host, port, username, password_enc, from_email, from_name, enabled, created_at)
    SELECT id, host, port, username, password_enc, from_email, from_name, enabled, created_at FROM smtp_config;

DROP TABLE smtp_config;
ALTER TABLE smtp_config_new RENAME TO smtp_config;
