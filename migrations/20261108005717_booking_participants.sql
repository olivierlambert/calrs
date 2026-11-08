-- booking_participants: the co-hosts of a dynamic group booking
-- (/u/alice+bob/slug), besides the event type owner. They used to be kept
-- only as attendee addresses, so a reschedule or an approval could not tell
-- which users had to be free and checked the owner alone (#46 review).
-- Bookings made before this migration have no rows and keep that behaviour.
CREATE TABLE booking_participants (
    booking_id TEXT NOT NULL REFERENCES bookings(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (booking_id, user_id)
);

CREATE INDEX idx_booking_participants_user ON booking_participants(user_id);
