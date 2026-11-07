-- When each user last signed in, for the admin panel (users list and the
-- "active users" figure).
--
-- Sessions cannot answer it on their own: signing out deletes the session
-- row, so a user who signs out looks as if they never signed in. Every sign
-- in (password, MFA, OIDC, CLI) creates a session, so a trigger on that
-- insert stamps the user without any call site having to remember it.
--
-- Backfill: the newest session each user still has.
ALTER TABLE users ADD COLUMN last_login_at TEXT;

UPDATE users SET last_login_at = (
    SELECT MAX(s.created_at) FROM sessions s WHERE s.user_id = users.id
);

CREATE TRIGGER IF NOT EXISTS sessions_stamp_last_login
AFTER INSERT ON sessions
BEGIN
    UPDATE users SET last_login_at = NEW.created_at WHERE id = NEW.user_id;
END;
