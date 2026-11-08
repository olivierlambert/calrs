-- ews_idp_verified
--
-- Where a trusted address got its trust: 1 when the identity provider
-- verified it (an OIDC login with email_verified, re-read at every login),
-- 0 when it is only an admin's word (`calrs user create`, a confirmation on
-- the Exchange card or `calrs user trust-email`). When two accounts map to
-- one Exchange mailbox, the IdP-verified owner takes it over from an account
-- that is only admin-confirmed, instead of being refused.
ALTER TABLE users ADD COLUMN email_verified_by_idp INTEGER NOT NULL DEFAULT 0;
