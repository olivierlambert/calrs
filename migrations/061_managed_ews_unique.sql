-- Make managed-EWS provisioning race-safe: at most one managed EWS source per
-- account. Concurrent provisioning (e.g. a user logging in via OIDC while the
-- admin clicks "provision now") could otherwise both pass a check-then-insert
-- and create duplicates. Remove any pre-existing duplicates (keep the
-- lexicographically smallest id) before enforcing the constraint.
DELETE FROM caldav_sources
WHERE managed = 1 AND provider_type = 'ews'
  AND id NOT IN (
    SELECT MIN(id) FROM caldav_sources
    WHERE managed = 1 AND provider_type = 'ews'
    GROUP BY account_id
  );

CREATE UNIQUE INDEX IF NOT EXISTS idx_caldav_sources_managed_ews_unique
  ON caldav_sources(account_id)
  WHERE managed = 1 AND provider_type = 'ews';
