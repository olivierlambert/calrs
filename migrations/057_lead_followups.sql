-- Lead-capture follow-up features + guest phone.
--
-- Guest phone: calrs previously had no notion of a guest phone number.
-- Event types can now opt in to collecting one on the public booking form
-- (and the lead-capture gate). Stored on bookings for completed bookings.
ALTER TABLE bookings ADD COLUMN guest_phone TEXT;
ALTER TABLE event_types ADD COLUMN collect_phone INTEGER NOT NULL DEFAULT 0;

-- Attribution: where the lead came from. Captured client-side from the
-- URL query (utm_*) and document.referrer at gate/booking-form load.
ALTER TABLE partial_bookings ADD COLUMN utm_source TEXT;
ALTER TABLE partial_bookings ADD COLUMN utm_medium TEXT;
ALTER TABLE partial_bookings ADD COLUMN utm_campaign TEXT;
ALTER TABLE partial_bookings ADD COLUMN referrer TEXT;

-- Worklist state set by the host from the dashboard. Archived rows drop
-- out of the default leads view; contacted_at drives a "contacted" badge.
ALTER TABLE partial_bookings ADD COLUMN contacted_at TEXT;
ALTER TABLE partial_bookings ADD COLUMN archived_at TEXT;

-- Set when the abandonment alert email was sent to the host, so the
-- background notifier emails at most once per abandoned lead.
ALTER TABLE partial_bookings ADD COLUMN notified_at TEXT;

CREATE INDEX IF NOT EXISTS idx_partial_bookings_archived
    ON partial_bookings(archived_at);
