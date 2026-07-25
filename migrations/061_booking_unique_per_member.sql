-- Round-robin team event types: one slot must be bookable once per assigned
-- member, not once per event type (#146). Rekey the double-booking guard by
-- the assigned member; NULL (personal event types, collective teams, dynamic
-- groups) keeps the old whole-slot exclusivity.
DROP INDEX IF EXISTS idx_bookings_no_overlap;
CREATE UNIQUE INDEX idx_bookings_no_overlap
ON bookings(event_type_id, start_at, COALESCE(assigned_user_id, ''))
WHERE status IN ('confirmed', 'pending');
