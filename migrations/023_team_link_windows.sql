-- Multiple availability windows for team links (same as event types)
-- Format: "09:00-12:00,13:00-17:00"
ALTER TABLE team_links ADD COLUMN availability_windows TEXT;
