-- When a booking was first confirmed (#266).
--
-- The additional attendees a guest invites get the "you've been added"
-- email, with the invite, once: when the booking becomes confirmed, at
-- submit or at the host's approval. A guest reschedule on an event type
-- that requires confirmation sends the booking back to pending, and its
-- approval looks exactly like the approval of a new request unless the
-- booking remembers it was confirmed before. Without that, attendees were
-- emailed again on every reschedule approval.
--
-- Set by the triggers below on every path that confirms a booking (web
-- booking, approval, CLI), so no call site has to remember it.
--
-- Backfill: bookings confirmed today are stamped with their creation time.
-- A pending booking can also be a confirmed one that its guest moved to a
-- time awaiting approval. Nothing records that directly, so a pending row
-- is stamped when it carries a trace that only a confirmation leaves:
--   meeting_url              auto meeting links are generated at confirmation
--   claimed_by_user_id, a booking_claim_tokens row
--                            watchers are offered a booking once confirmed
--   writeback_attempts, writeback_error, writeback_alerted_at
--                            recorded by calendar pushes, made only for
--                            confirmed bookings, and kept by the reschedule
-- A pending window opened after creation (pending_since) is not one: a
-- request its guest moved before any approval opens one too, and stamping
-- it would make its decline or expiry send attendees a cancellation for a
-- booking they never heard of.
-- Residual: a booking moved after confirmation with none of these traces
-- stays NULL. At approval its attendees get "added" once more and its guest
-- is texted "confirmed" rather than "moved"; if it is declined, expires or
-- is cancelled instead, neither its attendees nor its guest get a CANCEL
-- for the copy they do hold.
ALTER TABLE bookings ADD COLUMN first_confirmed_at TEXT;
UPDATE bookings SET first_confirmed_at = created_at
WHERE status = 'confirmed'
   OR (status = 'pending' AND (
        meeting_url IS NOT NULL
        OR claimed_by_user_id IS NOT NULL
        OR EXISTS (SELECT 1 FROM booking_claim_tokens t WHERE t.booking_id = bookings.id)
        OR writeback_attempts > 0
        OR writeback_error IS NOT NULL
        OR writeback_alerted_at IS NOT NULL
   ));
CREATE TRIGGER booking_first_confirmed_insert AFTER INSERT ON bookings
WHEN NEW.status = 'confirmed' AND NEW.first_confirmed_at IS NULL
BEGIN UPDATE bookings SET first_confirmed_at = datetime('now') WHERE id = NEW.id; END;
CREATE TRIGGER booking_first_confirmed_update AFTER UPDATE OF status ON bookings
WHEN NEW.status = 'confirmed' AND NEW.first_confirmed_at IS NULL
BEGIN UPDATE bookings SET first_confirmed_at = datetime('now') WHERE id = NEW.id; END;
