-- Three-level visibility: public (anyone), internal (authenticated users), private (invite-only)
ALTER TABLE event_types ADD COLUMN visibility TEXT NOT NULL DEFAULT 'public';
UPDATE event_types SET visibility = 'private' WHERE is_private = 1;
UPDATE event_types SET visibility = 'public' WHERE is_private = 0 OR is_private IS NULL;
