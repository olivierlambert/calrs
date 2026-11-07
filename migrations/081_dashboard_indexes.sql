-- Indexes for the 2.0 dashboard, admin panel and the busy-time reads on a
-- large instance (thousands of users, hundreds of thousands of bookings,
-- a million cached events), each checked with EXPLAIN QUERY PLAN. Indexes
-- only: no data changes.

-- The path from a user to their calendars: users -> accounts ->
-- caldav_sources -> calendars had no index on any of the foreign keys, so a
-- query scoped to one user could not start from that user and fell back to
-- scanning the whole events table by date.
CREATE INDEX IF NOT EXISTS idx_accounts_user ON accounts(user_id);
CREATE INDEX IF NOT EXISTS idx_caldav_sources_account ON caldav_sources(account_id);
CREATE INDEX IF NOT EXISTS idx_calendars_source ON calendars(source_id);

-- One calendar's events by date. `end_at` is in the key so the overlap test
-- (start before the window ends, end after it starts) is answered from the
-- index without reading past events. Replaces idx_events_calendar, its
-- prefix.
CREATE INDEX IF NOT EXISTS idx_events_calendar_start ON events(calendar_id, start_at, end_at);
DROP INDEX IF EXISTS idx_events_calendar;

-- Recurring masters, read on every busy-time computation whatever the
-- window (a series that started years ago still recurs).
CREATE INDEX IF NOT EXISTS idx_events_recurring ON events(calendar_id, start_at)
    WHERE rrule IS NOT NULL AND rrule != '';

-- Bookings by the user they belong to: the assigned member, the claimant,
-- or (through the event type) the owner and the team. The first two are
-- partial so that an `IS NULL` test (most bookings are neither assigned nor
-- claimed) can never be driven from them. The time columns and the status
-- ride along so a window over one user's bookings is filtered in the index
-- and only the matching rows are read. The event type index replaces
-- idx_bookings_event_type, its prefix.
CREATE INDEX IF NOT EXISTS idx_bookings_assigned_window
    ON bookings(assigned_user_id, start_at, status, end_at, created_at)
    WHERE assigned_user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_bookings_claimed ON bookings(claimed_by_user_id)
    WHERE claimed_by_user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_bookings_event_type_window
    ON bookings(event_type_id, start_at, status, end_at, created_at);
DROP INDEX IF EXISTS idx_bookings_event_type;

-- Bookings by date, now with their status, so counting the upcoming
-- confirmed ones needs no table read. Same leading column as before, so
-- every query that used it still does.
DROP INDEX IF EXISTS idx_bookings_start;
CREATE INDEX IF NOT EXISTS idx_bookings_start ON bookings(start_at, status);

-- Instance-wide figures of the admin panel, windowed by creation date.
-- The other columns ride along so the counts by status, the weekly chart
-- and the rankings read this index alone. No index leads with `status`:
-- with no statistics the planner would take it for any `status = ... AND
-- start_at < ...` and walk the instance's whole history instead of the
-- user's own bookings.
CREATE INDEX IF NOT EXISTS idx_bookings_created
    ON bookings(created_at, status, event_type_id, assigned_user_id);

-- Upcoming bookings whose calendar write-back failed, for the admin panel:
-- a handful of rows, found without reading every upcoming booking.
CREATE INDEX IF NOT EXISTS idx_bookings_writeback_failed ON bookings(start_at)
    WHERE writeback_error IS NOT NULL;

-- Claim links offered to a watcher, for their bookings page: read from the
-- watcher's own tokens rather than by walking every upcoming booking.
CREATE INDEX IF NOT EXISTS idx_booking_claim_tokens_user ON booking_claim_tokens(user_id);

-- The attendee count shown next to each booking was a scan of the whole
-- table per row.
CREATE INDEX IF NOT EXISTS idx_booking_attendees_booking ON booking_attendees(booking_id);

-- Weekly rules and date overrides are read per event type (slot pages, the
-- overview's bookable hours, one query per event type), each a full scan.
CREATE INDEX IF NOT EXISTS idx_availability_rules_event_type ON availability_rules(event_type_id);
CREATE INDEX IF NOT EXISTS idx_availability_overrides_event_type ON availability_overrides(event_type_id);
