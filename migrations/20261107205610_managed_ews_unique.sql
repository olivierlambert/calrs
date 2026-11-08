-- Make managed-EWS provisioning race-safe: at most one managed EWS source per
-- account. Concurrent provisioning (e.g. a user logging in via OIDC while the
-- admin clicks "provision now") could otherwise both pass a check-then-insert
-- and create duplicates.
--
-- No database that shipped has managed rows yet (the column arrives in
-- 20261107205608 with default 0), and 20261107211215_ews_email_trust removes
-- every managed row anyway, since no account is trusted before it. So rather
-- than merging duplicates, any managed EWS row a development database holds
-- goes now, so the unique index below can always be built. Their calendars,
-- cached events and event-type links cascade; provisioning recreates them.
DELETE FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews';

CREATE UNIQUE INDEX IF NOT EXISTS idx_caldav_sources_managed_ews_unique
  ON caldav_sources(account_id)
  WHERE managed = 1 AND provider_type = 'ews';
