-- ews_email_trust
--
-- A managed Exchange source opens the mailbox its user's address names, so
-- calrs only provisions one for an address it can trust: an account an admin
-- created (`calrs user create`), an SSO account whose email the identity
-- provider marks verified (re-checked at every login), or an account an admin
-- confirmed by hand. Local self-registration never sets it.
--
-- Accounts that exist before this migration are not trusted until an admin
-- confirms them once, from the Exchange card or with
-- `calrs user trust-email <email>`.
ALTER TABLE users ADD COLUMN email_trusted INTEGER NOT NULL DEFAULT 0;

-- The mailbox-domain override now only rewrites addresses in this one domain
-- (the users' own domain, e.g. example.local); every other address is used
-- as-is. The override applies only while both domains are set.
ALTER TABLE auth_config ADD COLUMN ews_impersonation_from_domain TEXT;

-- Every existing account starts untrusted, so no managed source may stay
-- attached to one. An admin's confirmation provisions the user again. Their
-- calendars and cached events go with the rows (ON DELETE CASCADE).
DELETE FROM caldav_sources WHERE managed = 1 AND provider_type = 'ews';

-- impersonate_email now holds the mailbox actually impersonated, lowercased
-- (after the domain override), and one mailbox serves at most one user.
CREATE UNIQUE INDEX IF NOT EXISTS idx_caldav_sources_managed_ews_mailbox
  ON caldav_sources(impersonate_email)
  WHERE managed = 1 AND provider_type = 'ews';
