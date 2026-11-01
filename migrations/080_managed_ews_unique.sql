-- Make managed-EWS provisioning race-safe: at most one managed EWS source per
-- account. Concurrent provisioning (e.g. a user logging in via OIDC while the
-- admin clicks "provision now") could otherwise both pass a check-then-insert
-- and create duplicates.
--
-- Duplicates are merged, not just dropped: calendars and events hang off a
-- source with ON DELETE CASCADE, and users' event types can be linked to those
-- calendars, so deleting a duplicate outright would take that with it. The
-- survivor is the row the user has set up (a write calendar), then the one
-- holding the most calendars, then the smallest id. Calendars the survivor
-- lacks move over; for a calendar both have, the links and cached events move
-- onto the survivor's copy. Whatever is left on a duplicate is a copy of
-- something the survivor already has, and goes with it.
CREATE TEMP TABLE _managed_ews_survivor AS
SELECT account_id,
       (SELECT s2.id FROM caldav_sources s2
         WHERE s2.account_id = s.account_id AND s2.managed = 1 AND s2.provider_type = 'ews'
         ORDER BY (s2.write_calendar_href IS NOT NULL) DESC,
                  (SELECT COUNT(*) FROM calendars c WHERE c.source_id = s2.id) DESC,
                  s2.id
         LIMIT 1) AS survivor_id
FROM caldav_sources s
WHERE s.managed = 1 AND s.provider_type = 'ews'
GROUP BY s.account_id
HAVING COUNT(*) > 1;

-- Calendars the survivor does not have yet move to it.
UPDATE calendars
SET source_id = (SELECT survivor_id FROM _managed_ews_survivor m
                  JOIN caldav_sources d ON d.account_id = m.account_id
                  WHERE d.id = calendars.source_id)
WHERE source_id IN (SELECT d.id FROM caldav_sources d
                     JOIN _managed_ews_survivor m ON m.account_id = d.account_id
                     WHERE d.managed = 1 AND d.provider_type = 'ews' AND d.id != m.survivor_id)
  AND NOT EXISTS (
    SELECT 1 FROM calendars keep
    WHERE keep.href = calendars.href
      AND keep.source_id = (SELECT survivor_id FROM _managed_ews_survivor m
                             JOIN caldav_sources d ON d.account_id = m.account_id
                             WHERE d.id = calendars.source_id));

-- Calendars both rows have: point links and cached events at the survivor's.
CREATE TEMP TABLE _managed_ews_calendar_map AS
SELECT dup.id AS dup_calendar_id, keep.id AS keep_calendar_id
FROM calendars dup
JOIN caldav_sources d ON d.id = dup.source_id
JOIN _managed_ews_survivor m ON m.account_id = d.account_id
JOIN calendars keep ON keep.source_id = m.survivor_id AND keep.href = dup.href
WHERE d.managed = 1 AND d.provider_type = 'ews' AND d.id != m.survivor_id;

INSERT OR IGNORE INTO event_type_calendars (event_type_id, calendar_id)
SELECT etc.event_type_id, map.keep_calendar_id
FROM event_type_calendars etc
JOIN _managed_ews_calendar_map map ON map.dup_calendar_id = etc.calendar_id;

UPDATE OR IGNORE events
SET calendar_id = (SELECT keep_calendar_id FROM _managed_ews_calendar_map
                    WHERE dup_calendar_id = events.calendar_id)
WHERE calendar_id IN (SELECT dup_calendar_id FROM _managed_ews_calendar_map);

DELETE FROM caldav_sources
WHERE managed = 1 AND provider_type = 'ews'
  AND id IN (SELECT d.id FROM caldav_sources d
              JOIN _managed_ews_survivor m ON m.account_id = d.account_id
              WHERE d.id != m.survivor_id);

DROP TABLE _managed_ews_calendar_map;
DROP TABLE _managed_ews_survivor;

CREATE UNIQUE INDEX IF NOT EXISTS idx_caldav_sources_managed_ews_unique
  ON caldav_sources(account_id)
  WHERE managed = 1 AND provider_type = 'ews';
