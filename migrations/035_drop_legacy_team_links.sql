-- Drop legacy team_links tables, now replaced by the unified teams system (migration 034).
-- Data was migrated to teams/team_members/event_types/bookings by migrate_team_links_to_teams().
--
-- Note: event_types.group_id is left in place because SQLite cannot DROP COLUMN.
-- It is unused by new code (superseded by team_id) and can be ignored.

DROP TABLE IF EXISTS team_link_bookings;
DROP TABLE IF EXISTS team_link_members;
DROP TABLE IF EXISTS team_links;
