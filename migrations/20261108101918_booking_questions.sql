-- booking_questions
--
-- Booking questions (#210, #170): the host asks guests extra questions on the
-- booking form, per event type, and each booking keeps a snapshot of what
-- the guest read and answered.
--
-- `event_type_questions` is the form: one row per question, in `position`
-- order. `kind` is one of short_text, long_text, single_choice,
-- multiple_choice, checkbox (text, so a later kind needs no migration).
-- `key` is a stable identifier ([a-z0-9_-]) for webhooks and later prefill,
-- derived from the label when the question is created and kept when it is
-- renamed. `options` is a JSON array of {"id", "label"} for the two choice
-- kinds, ids stable across renames. `consent` marks a checkbox as a consent
-- (a policy agreement), `link_url` the policy it links to. `in_calendar = 0`
-- keeps the answer out of the host's calendar event, which delegates may
-- read.
CREATE TABLE event_type_questions (
    id            TEXT PRIMARY KEY,
    event_type_id TEXT NOT NULL REFERENCES event_types(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL,
    key           TEXT NOT NULL,
    kind          TEXT NOT NULL,
    label         TEXT NOT NULL,
    help_text     TEXT,
    required      INTEGER NOT NULL DEFAULT 0,
    options       TEXT,
    consent       INTEGER NOT NULL DEFAULT 0,
    link_url      TEXT,
    in_calendar   INTEGER NOT NULL DEFAULT 1,
    UNIQUE (event_type_id, key)
);
CREATE INDEX idx_event_type_questions_et ON event_type_questions(event_type_id, position);

-- One row per answered question of a booking. Label, link and flags are a
-- snapshot of the question as the guest saw it, so editing or deleting the
-- question never rewrites a past booking (`question_id` then becomes NULL).
-- `value` is the display text ("yes"/"no" for a checkbox, the option labels
-- for a choice), `value_ids` the JSON array of chosen option ids. An
-- optional question left empty stores nothing, except a checkbox, whose
-- "no" is kept: "did not agree to the recording" matters to the host.
-- `answered_at` is when the booking form was submitted (UTC).
CREATE TABLE booking_answers (
    id          TEXT PRIMARY KEY,
    booking_id  TEXT NOT NULL REFERENCES bookings(id) ON DELETE CASCADE,
    question_id TEXT REFERENCES event_type_questions(id) ON DELETE SET NULL,
    position    INTEGER NOT NULL,
    key         TEXT NOT NULL,
    kind        TEXT NOT NULL,
    label       TEXT NOT NULL,
    value       TEXT NOT NULL,
    value_ids   TEXT,
    consent     INTEGER NOT NULL DEFAULT 0,
    link_url    TEXT,
    in_calendar INTEGER NOT NULL DEFAULT 1,
    answered_at TEXT NOT NULL
);
CREATE INDEX idx_booking_answers_booking ON booking_answers(booking_id);
-- Deleting a question sets `question_id` NULL on its answers: without this
-- index that would scan every answer of the instance.
CREATE INDEX idx_booking_answers_question ON booking_answers(question_id);

-- The built-in notes field: 'off' hides it, 'optional' (as before) shows it,
-- 'required' makes the guest fill it in.
ALTER TABLE event_types ADD COLUMN notes_mode TEXT NOT NULL DEFAULT 'optional';
