-- #223: mark the attendee row a claim adds for the watcher who claimed the
-- booking, so their copies of a later reschedule or cancellation never get
-- the guest footer, and read in their own language rather than the booking's.
--
-- added_by_claim is the role and outlives the account: deleting the claimant
-- nulls claimant_user_id (and bookings.claimed_by_user_id), but the address
-- on the row is still staff, not a guest. claimant_user_id says whose row it
-- is while that user exists, so a later change of their booking email does
-- not lose them.
ALTER TABLE booking_attendees ADD COLUMN added_by_claim INTEGER NOT NULL DEFAULT 0;
ALTER TABLE booking_attendees ADD COLUMN claimant_user_id TEXT REFERENCES users(id) ON DELETE SET NULL;

-- Claims made before this migration. A guest's attendees are inserted with
-- the booking, and only a confirmed booking can be claimed, so they predate
-- bookings.claimed_at; the claim's row was inserted right after the claim,
-- and both timestamps come from datetime('now'). A row not provably older
-- than the claim is marked: that is the claimant's, under whatever address
-- they had then, or, for a row created within the same second as the claim,
-- possibly the claimant's, which then goes without the footer rather than
-- risk sending it to staff. A booking whose claimant was deleted keeps
-- claimed_at, so its row is still found.
UPDATE booking_attendees
SET added_by_claim = 1
WHERE EXISTS (
    SELECT 1
    FROM bookings b
    WHERE b.id = booking_attendees.booking_id
      AND (b.claimed_by_user_id IS NOT NULL OR b.claimed_at IS NOT NULL)
      AND COALESCE(julianday(booking_attendees.created_at) < julianday(b.claimed_at), 0) = 0
);

-- The claimant's own row, while they exist: the marked row under their
-- current address, else the booking's only marked row. Several marked rows
-- under other addresses cannot be told apart; they stay without a user and
-- read the booking's language, but still get no footer.
UPDATE booking_attendees
SET claimant_user_id = (
    SELECT b.claimed_by_user_id FROM bookings b WHERE b.id = booking_attendees.booking_id
)
WHERE added_by_claim = 1
  AND EXISTS (
    SELECT 1
    FROM bookings b
    JOIN users u ON u.id = b.claimed_by_user_id
    WHERE b.id = booking_attendees.booking_id
      AND (
        LOWER(COALESCE(u.booking_email, u.email)) = LOWER(booking_attendees.email)
        OR NOT EXISTS (
          SELECT 1 FROM booking_attendees other
          WHERE other.booking_id = b.id
            AND other.added_by_claim = 1
            AND other.id <> booking_attendees.id
        )
      )
  );
