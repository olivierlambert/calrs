-- Existing rows retain their exact timestamps and legacy interpretation.
ALTER TABLE bookings ADD COLUMN time_version INTEGER NOT NULL DEFAULT 0
    CHECK (time_version IN (0, 1));

-- New writes explicitly select version 1 and use UTC with a trailing Z.
CREATE TRIGGER booking_utc_insert BEFORE INSERT ON bookings
WHEN NEW.time_version = 1 AND
    (NEW.start_at NOT GLOB '????-??-??T??:??:??Z' OR NEW.end_at NOT GLOB '????-??-??T??:??:??Z')
BEGIN SELECT RAISE(ABORT, 'UTC bookings require UTC timestamps'); END;
CREATE TRIGGER booking_utc_update BEFORE UPDATE OF start_at, end_at, time_version ON bookings
WHEN NEW.time_version = 1 AND
    (NEW.start_at NOT GLOB '????-??-??T??:??:??Z' OR NEW.end_at NOT GLOB '????-??-??T??:??:??Z')
BEGIN SELECT RAISE(ABORT, 'UTC bookings require UTC timestamps'); END;
