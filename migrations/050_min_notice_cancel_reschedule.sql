-- Per-event-type minimum notice window for guest-initiated cancel and reschedule.
-- NULL or 0 means no restriction (the existing behaviour).
-- Values are stored in minutes; the form lets the user pick minutes/hours/days
-- and converts to minutes before INSERT/UPDATE.
ALTER TABLE event_types ADD COLUMN cancel_notice_min INTEGER;
ALTER TABLE event_types ADD COLUMN reschedule_notice_min INTEGER;
