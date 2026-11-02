-- Display order only. Availability day_of_week remains 0=Sunday ... 6=Saturday.
ALTER TABLE auth_config ADD COLUMN week_start TEXT NOT NULL DEFAULT 'mon'
    CHECK (week_start IN ('sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat'));
ALTER TABLE users ADD COLUMN week_start TEXT
    CHECK (week_start IN ('sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat'));
ALTER TABLE event_types ADD COLUMN week_start TEXT
    CHECK (week_start IN ('sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat'));
