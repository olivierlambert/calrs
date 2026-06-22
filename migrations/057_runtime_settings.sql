-- Runtime settings that were previously env-only, now overridable from the DB
-- (admin UI / `calrs config general`). The environment variable still takes
-- precedence at runtime when set — see `src/settings.rs`.
--
-- base_url            ← CALRS_BASE_URL (public URL for OIDC redirects & email links)
-- allow_private_hosts ← CALRS_ALLOW_PRIVATE_HOSTS (comma-separated SSRF allowlist)
ALTER TABLE auth_config ADD COLUMN base_url TEXT;
ALTER TABLE auth_config ADD COLUMN allow_private_hosts TEXT;
