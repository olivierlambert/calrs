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

-- Claims made before this migration. A guest's attendees are only ever
-- inserted in the transaction that creates the booking, so their created_at
-- is within seconds of bookings.created_at (both from datetime('now')). A
-- claim needs a confirmed booking, a watcher email and a click, so its row
-- comes later. On a claimed booking, every attendee row not provably a
-- guest's invitee is marked: one created more than 10 seconds after the
-- booking was, which covers every claim the booking had (a first claimant
-- since deleted, then a second one), under whatever address each had then.
-- A row whose timestamps cannot be compared is marked too: it goes without
-- the footer rather than risk sending it to staff. A booking whose
-- claimant was deleted keeps claimed_at, so its rows are still found.
UPDATE booking_attendees
SET added_by_claim = 1
WHERE EXISTS (
    SELECT 1
    FROM bookings b
    WHERE b.id = booking_attendees.booking_id
      AND (b.claimed_by_user_id IS NOT NULL OR b.claimed_at IS NOT NULL)
      AND COALESCE(
            (julianday(booking_attendees.created_at) - julianday(b.created_at)) * 86400 <= 10,
            0
          ) = 0
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
