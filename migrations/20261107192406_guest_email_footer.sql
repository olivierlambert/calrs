-- #223: optional footer appended to every email sent to a guest.
-- One text for all languages, a markdown subset (bold, italic, links).
-- An event type's own footer replaces the instance default.
ALTER TABLE event_types ADD COLUMN guest_email_footer TEXT;
ALTER TABLE auth_config ADD COLUMN guest_email_footer TEXT;
-- Admins may always edit footers; with this on, every user may on their own
-- event types (same shape as sms_allow_all_users).
ALTER TABLE auth_config ADD COLUMN guest_footer_allow_all_users INTEGER NOT NULL DEFAULT 0;
