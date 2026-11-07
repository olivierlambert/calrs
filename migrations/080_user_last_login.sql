-- When each user last signed in, for the admin panel (users list and the
-- "active users" figure).
--
-- Sessions cannot answer it on their own: signing out deletes the session
-- row, so a user who signs out looks as if they never signed in. Every sign
-- in (password, MFA, OIDC, CLI) creates a session, so triggers on sessions
-- stamp the user without any call site having to remember it.
--
-- A sign in counts once it is complete. Sessions are always inserted with
-- mfa_verified = 0; when MFA applies (a local account with MFA enrolled, or
-- any local account while the admin requires MFA), the session only counts
-- once mfa.rs flips mfa_verified to 1 after the code is checked, so the
-- insert stamps nothing and the update does. Without MFA (OIDC accounts,
-- local accounts with nothing enrolled and nothing required) the insert is
-- the sign in.
--
-- Backfill: the newest session each user still has.
ALTER TABLE users ADD COLUMN last_login_at TEXT;

UPDATE users SET last_login_at = (
    SELECT MAX(s.created_at) FROM sessions s WHERE s.user_id = users.id
);

CREATE TRIGGER IF NOT EXISTS sessions_stamp_last_login
AFTER INSERT ON sessions
WHEN NEW.mfa_verified = 1
  OR NOT EXISTS (
      SELECT 1 FROM users u WHERE u.id = NEW.user_id AND u.auth_provider = 'local'
        AND (EXISTS (SELECT 1 FROM user_mfa m WHERE m.user_id = u.id)
             OR COALESCE((SELECT mfa_required FROM auth_config WHERE id = 'singleton'), 0) != 0))
BEGIN
    UPDATE users SET last_login_at = NEW.created_at WHERE id = NEW.user_id;
END;

CREATE TRIGGER IF NOT EXISTS sessions_stamp_last_login_mfa
AFTER UPDATE OF mfa_verified ON sessions
WHEN NEW.mfa_verified = 1 AND OLD.mfa_verified = 0
BEGIN
    UPDATE users SET last_login_at = datetime('now') WHERE id = NEW.user_id;
END;
