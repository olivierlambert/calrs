-- Some Exchange deployments use a different domain for the AD UPN / calrs
-- login email (e.g. dyb.lan) than for the mailbox PrimarySmtpAddress
-- (e.g. dyb.fr). When set, the impersonation target's domain is rewritten to
-- this value so <t:PrimarySmtpAddress> resolves to a real mailbox.
ALTER TABLE auth_config ADD COLUMN ews_impersonation_domain TEXT;
