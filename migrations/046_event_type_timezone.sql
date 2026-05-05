-- Explicit timezone for event types. Availability rules (09:00-17:00 etc.)
-- are interpreted in this timezone. NULL preserves the legacy fallback
-- in get_host_tz: use the account owner's user timezone.
ALTER TABLE event_types ADD COLUMN timezone TEXT;

-- Backfill existing rows with the account owner's timezone so the interpreted
-- host_tz does not change for deployments upgrading past this migration.
UPDATE event_types
SET timezone = (
    SELECT u.timezone FROM users u
    JOIN accounts a ON a.user_id = u.id
    WHERE a.id = event_types.account_id
)
WHERE timezone IS NULL;
