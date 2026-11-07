-- #223: mark the attendee row a claim adds for the watcher who claimed the
-- booking. The row keeps the address the claimant had at claim time, so a
-- later change of their booking email can no longer make calrs mistake them
-- for a guest-invited attendee (their copies would get the guest footer and
-- the booking's language instead of their own).
ALTER TABLE booking_attendees ADD COLUMN claimant_user_id TEXT REFERENCES users(id) ON DELETE SET NULL;

-- Claims made before this migration: the claimant's row is the one under
-- their current address. A claimant who changed it since cannot be told
-- apart from a guest-invited attendee and stays unmarked.
UPDATE booking_attendees
SET claimant_user_id = (
    SELECT b.claimed_by_user_id
    FROM bookings b
    JOIN users u ON u.id = b.claimed_by_user_id
    WHERE b.id = booking_attendees.booking_id
      AND LOWER(COALESCE(u.booking_email, u.email)) = LOWER(booking_attendees.email)
)
WHERE claimant_user_id IS NULL
  AND EXISTS (
    SELECT 1
    FROM bookings b
    JOIN users u ON u.id = b.claimed_by_user_id
    WHERE b.id = booking_attendees.booking_id
      AND LOWER(COALESCE(u.booking_email, u.email)) = LOWER(booking_attendees.email)
  );
