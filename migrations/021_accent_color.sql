-- Admin-configurable accent color theme
ALTER TABLE auth_config ADD COLUMN accent_color TEXT NOT NULL DEFAULT 'blue';
