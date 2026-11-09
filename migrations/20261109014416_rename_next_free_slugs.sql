-- rename_next_free_slugs
-- Intentionally a no-op. This migration once renamed event types holding
-- the slug `next-free`, which the landing pages' next-free hints were to
-- reserve. Those hints were dropped before any release, so the slug is no
-- longer reserved and nothing is renamed. The file stays so every database
-- records the name once (deleting a migration from a branch is refused).
-- A development or demo database that ran the earlier version keeps the
-- slug it was given (`next-free-1` or similar); none ever shipped in a tag.
SELECT 1;
