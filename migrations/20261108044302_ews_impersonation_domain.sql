-- Some Exchange deployments use a different domain for the AD UPN / calrs
-- login email (e.g. example.local) than for the mailbox addresses
-- (e.g. example.com). When set, the impersonation target's domain is rewritten
-- to this value so <t:SmtpAddress> resolves to a real mailbox.
ALTER TABLE auth_config ADD COLUMN ews_impersonation_domain TEXT;
