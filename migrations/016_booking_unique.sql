-- Prevent double-booking race conditions: no two confirmed/pending bookings
-- for the same event type can start at the same time.
CREATE UNIQUE INDEX IF NOT EXISTS idx_bookings_no_overlap
ON bookings(event_type_id, start_at)
WHERE status IN ('confirmed', 'pending');
