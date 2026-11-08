-- orphan_check_inconclusive
-- When a Microsoft source's orphan checks started coming back
-- inconclusive (Graph could not say whether a vanished event is gone),
-- cleared by a sync with none. Past a day the sync reports it, so the
-- owner learns that deletions on that source no longer cancel bookings.
ALTER TABLE caldav_sources ADD COLUMN orphan_check_inconclusive_since TEXT;
