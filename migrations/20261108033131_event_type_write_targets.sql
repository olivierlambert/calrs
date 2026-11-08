-- event_type_write_targets (#47)
--
-- Where a host's confirmed bookings of one event type are written, instead
-- of the write calendar of each of their sources. One row per (event type,
-- user): the user is the host whose own calendar it is, which for a team
-- event type means every member chooses their own. No row means the source
-- setting, exactly as before. The calendar must be one of the user's own
-- (checked when saved, and again when read). Removing the calendar's source
-- removes the row, and the bookings go back to the source setting.
CREATE TABLE event_type_write_targets (
    event_type_id TEXT NOT NULL REFERENCES event_types(id) ON DELETE CASCADE,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    calendar_id   TEXT NOT NULL REFERENCES calendars(id) ON DELETE CASCADE,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (event_type_id, user_id)
);

CREATE INDEX idx_event_type_write_targets_calendar
    ON event_type_write_targets(calendar_id);

-- The copies of a booking written to such a destination, per host. A
-- collective booking has one copy per member, which the single
-- bookings.caldav_calendar_href cannot describe, and a destination can change
-- while a booking is upcoming: these rows let a later push, cancellation or
-- reschedule find and remove the copy where it was actually written. Pushes
-- to the source setting record nothing here, so instances that never choose
-- a destination keep their previous behaviour.
CREATE TABLE booking_write_copies (
    booking_uid   TEXT NOT NULL REFERENCES bookings(uid) ON DELETE CASCADE,
    user_id       TEXT NOT NULL,
    source_id     TEXT NOT NULL REFERENCES caldav_sources(id) ON DELETE CASCADE,
    calendar_href TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (booking_uid, user_id, source_id, calendar_href)
);
