-- booking_webhooks
--
-- Outgoing webhooks for booking events (#168). An endpoint belongs either to
-- one event type (`event_type_id` set: managed by whoever may manage that
-- event type) or to a user (`event_type_id` NULL: every personal event type
-- of `user_id`). `user_id` is the user who added it, or who took it over.
-- A webhook only sends while that user can still manage its scope (checked
-- live, see ACTIVE_ENDPOINT in src/web/webhooks.rs); otherwise it is
-- suspended until a current manager takes it over. When the user is
-- deleted it becomes NULL and an event type's webhook stays, suspended (the
-- trigger below removes a user's own "all my event types" webhooks with
-- them). `events`
-- lists the events sent, comma separated; NULL means all of them. The
-- secret signs every request (HMAC-SHA256, the meeting webhook's scheme)
-- and is stored encrypted with the instance key, like every other
-- credential.
CREATE TABLE webhooks (
    id                TEXT PRIMARY KEY,
    user_id           TEXT REFERENCES users(id) ON DELETE SET NULL,
    event_type_id     TEXT REFERENCES event_types(id) ON DELETE CASCADE,
    url               TEXT NOT NULL,
    secret_enc        TEXT NOT NULL,
    events            TEXT,
    enabled           INTEGER NOT NULL DEFAULT 1,
    last_attempt_at   TEXT,
    last_status       INTEGER,
    last_error        TEXT,
    last_success_at   TEXT,
    created_at        TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_webhooks_event_type ON webhooks(event_type_id);
CREATE INDEX idx_webhooks_user ON webhooks(user_id);

-- Runs before the foreign key sets `user_id` to NULL on the rest.
CREATE TRIGGER webhooks_follow_deleted_user BEFORE DELETE ON users
BEGIN
    DELETE FROM webhooks WHERE user_id = OLD.id AND event_type_id IS NULL;
END;

-- The delivery queue. A row is written after the booking change commits and
-- delivered by the background webhook task. `id` is the idempotency id sent
-- with every attempt; `seq` keeps the deliveries of one booking in order.
-- A delivered row is deleted. One that ran out of attempts, or was still
-- not delivered after 24 hours, is kept a week as `failed`, then pruned.
-- The pending rows of a webhook that stops sending (switched off,
-- suspended, its user disabled, or the admin switch turned off) are
-- deleted, so guest data neither piles up here nor replays weeks later.
CREATE TABLE webhook_deliveries (
    seq               INTEGER PRIMARY KEY AUTOINCREMENT,
    id                TEXT NOT NULL UNIQUE,
    webhook_id        TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    booking_uid       TEXT NOT NULL,
    event             TEXT NOT NULL,
    payload           TEXT NOT NULL,
    state             TEXT NOT NULL DEFAULT 'pending',
    attempts          INTEGER NOT NULL DEFAULT 0,
    next_attempt_at   TEXT NOT NULL,
    last_error        TEXT,
    created_at        TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_webhook_deliveries_due ON webhook_deliveries(state, next_attempt_at);
CREATE INDEX idx_webhook_deliveries_webhook ON webhook_deliveries(webhook_id, booking_uid);

-- Who may add webhooks: admins only by default, like SMS. A webhook sends
-- guest names, emails and phone numbers to a URL of the user's choosing.
ALTER TABLE auth_config ADD COLUMN webhooks_allow_all_users INTEGER NOT NULL DEFAULT 0;
