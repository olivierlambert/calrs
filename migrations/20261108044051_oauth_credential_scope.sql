-- NULL retains the grant scope for credentials issued before this migration.
-- An access token can have fewer permissions than its refresh grant.
ALTER TABLE oauth_credentials ADD COLUMN scope TEXT;
