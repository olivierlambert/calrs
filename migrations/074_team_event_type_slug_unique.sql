-- Team event type slugs are unique per team (#313). The table only enforces
-- UNIQUE(account_id, slug), and a team event type is stored under its
-- creator's account, so two members could each create one with the same slug
-- in the same team, making /team/{team}/{slug} ambiguous.
--
-- Repair first: in each team, every event type sharing a slug with an older
-- one gets the start of its id appended, so the oldest keeps the URL.
UPDATE event_types
SET slug = slug || '-' || substr(id, 1, 8)
WHERE team_id IS NOT NULL
  AND EXISTS (
    SELECT 1 FROM event_types AS older
    WHERE older.team_id = event_types.team_id
      AND older.slug = event_types.slug
      AND (COALESCE(older.created_at, '') < COALESCE(event_types.created_at, '')
           OR (COALESCE(older.created_at, '') = COALESCE(event_types.created_at, '')
               AND older.id < event_types.id))
  );

CREATE UNIQUE INDEX IF NOT EXISTS idx_event_types_team_slug
    ON event_types(team_id, slug) WHERE team_id IS NOT NULL;
