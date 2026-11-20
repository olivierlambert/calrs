-- booking_attribution: the utm_* parameters of the link a guest arrived by,
-- one row per booking that carried any (src/web/attribution.rs). Hosts only.
CREATE TABLE booking_attribution (
    booking_id   TEXT PRIMARY KEY REFERENCES bookings(id) ON DELETE CASCADE,
    utm_source   TEXT,
    utm_medium   TEXT,
    utm_campaign TEXT,
    utm_term     TEXT,
    utm_content  TEXT
);
