-- rename_next_free_slugs
-- The landing pages answer their next-free hints at /u/{username}/next-free
-- and /team/{slug}/next-free, so an event type with the slug `next-free` is
-- unreachable there and the slug is now reserved. An event type that already
-- holds it is renamed to the first free `next-free-N`: free in its account
-- (slugs are unique per account) and, for a team event type, in its team
-- (#313). Two `next-free` rows never share an account or a team, so they
-- cannot compete for the same new slug.
WITH RECURSIVE n(i) AS (
    SELECT 1
    UNION ALL
    SELECT i + 1 FROM n WHERE i < 1000
)
UPDATE event_types
SET slug = COALESCE(
    (SELECT 'next-free-' || n.i
     FROM n
     WHERE NOT EXISTS (
         SELECT 1 FROM event_types AS other
         WHERE other.slug = 'next-free-' || n.i
           AND (other.account_id = event_types.account_id
                OR (event_types.team_id IS NOT NULL
                    AND other.team_id = event_types.team_id))
     )
     ORDER BY n.i
     LIMIT 1),
    'next-free-' || substr(id, 1, 8)
)
WHERE slug = 'next-free';
