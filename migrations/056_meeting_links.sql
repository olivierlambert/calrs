-- Auto-generated video meeting links (issue #45).
--
-- Two providers ship in this migration:
--   * `jitsi_auto` — a fresh room is computed per booking from a configurable
--     pattern of tokens ({username}, {event}, {date}, {random}) and appended
--     to a base URL (e.g. https://meet.dyb.fr).
--   * `webhook_auto` — calrs POSTs the booking payload to a configured URL
--     when the booking is confirmed and expects `{"url": "..."}` back.
--
-- The generated URL is stored per-booking on `bookings.meeting_url` so the
-- guest, host, ICS attachment, and CalDAV write-back all see the same value
-- without recomputing (which could otherwise produce a different {random}).
--
-- Both providers can be defaulted at the auth_config (org) level and
-- overridden per event_type via `meeting_pattern_override`.

-- Org-wide defaults. Empty/NULL means the provider is not configured.
ALTER TABLE auth_config ADD COLUMN jitsi_base_url TEXT;
ALTER TABLE auth_config ADD COLUMN jitsi_pattern TEXT;
-- Optional human-readable label shown to guests instead of the generic
-- "Video call" badge — e.g. "Meet DYB". NULL/empty falls back to a generic
-- label so the UI works whether or not the admin has branded the provider.
ALTER TABLE auth_config ADD COLUMN jitsi_display_name TEXT;

ALTER TABLE auth_config ADD COLUMN meeting_webhook_url TEXT;
-- 'none' or 'hmac'. Persisted as plain text; the actual key lives in
-- meeting_webhook_secret (encrypted at rest like other auth_config secrets).
ALTER TABLE auth_config ADD COLUMN meeting_webhook_auth_mode TEXT;
ALTER TABLE auth_config ADD COLUMN meeting_webhook_secret TEXT;
ALTER TABLE auth_config ADD COLUMN meeting_webhook_display_name TEXT;

-- Per event_type override of the Jitsi pattern. NULL = use org default.
-- The provider itself is encoded in the existing `location_type` column,
-- which gains two new accepted values: 'jitsi_auto' and 'webhook_auto'.
ALTER TABLE event_types ADD COLUMN meeting_pattern_override TEXT;

-- Per-booking generated URL. NULL means either the event type is a static
-- location (use location_value) or generation has not happened yet.
ALTER TABLE bookings ADD COLUMN meeting_url TEXT;
