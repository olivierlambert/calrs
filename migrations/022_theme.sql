-- Full theme support: preset themes + custom colors
-- Rename accent_color to theme (default, nord, dracula, custom)
-- Add custom color columns for user-defined themes
ALTER TABLE auth_config ADD COLUMN theme TEXT NOT NULL DEFAULT 'default';
ALTER TABLE auth_config ADD COLUMN custom_accent TEXT;
ALTER TABLE auth_config ADD COLUMN custom_accent_hover TEXT;
ALTER TABLE auth_config ADD COLUMN custom_bg TEXT;
ALTER TABLE auth_config ADD COLUMN custom_surface TEXT;
ALTER TABLE auth_config ADD COLUMN custom_text TEXT;
