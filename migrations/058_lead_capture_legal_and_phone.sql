-- Lead capture follow-ups + phone-number obligation level.
--
-- Legal mentions URL: lets the admin replace the verbose RGPD micro-notice
-- that appears next to the booking form with a single link to their own
-- privacy / legal-mentions page. Stored on auth_config (singleton).
ALTER TABLE auth_config ADD COLUMN legal_mentions_url TEXT;

-- Acknowledgement timestamp for the per-event-type "I have informed bookers"
-- gate. Once a host has confirmed once for an event type, the dashboard can
-- stop nagging them with a separate checkbox: the gate is implicit on
-- subsequent enable/disable toggles.
ALTER TABLE event_types ADD COLUMN lead_capture_acknowledged_at TEXT;

-- Phone obligation level:
--   0 = not collected (default, matches pre-existing behaviour)
--   1 = collected, optional (was "1" before, semantics unchanged)
--   2 = collected, required (new)
-- The column already exists as INTEGER (migration 057), so no schema change
-- is needed — this comment records the new semantics for INT values >= 2.
-- Old rows storing 1 keep their "optional" behaviour.
