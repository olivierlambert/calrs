-- trusted_proxies
-- The TCP peers whose X-Forwarded-For is believed when telling clients
-- apart for the rate limits (#85). NULL is the default list (loopback,
-- private, unique local and link-local ranges), 'none' trusts no peer;
-- CALRS_TRUSTED_PROXIES overrides it.
ALTER TABLE auth_config ADD COLUMN trusted_proxies TEXT;
