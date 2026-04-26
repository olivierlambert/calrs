-- Per-user UI language preference. NULL = follow Accept-Language header.
ALTER TABLE users ADD COLUMN language TEXT;
