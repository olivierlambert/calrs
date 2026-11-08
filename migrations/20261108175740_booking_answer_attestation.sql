-- A consent answered through the REST API (#169, #210) is the caller's
-- attestation, not the guest's own tick: the answer row, which is the
-- consent evidence, records who made it and with which credential. No
-- foreign keys: the evidence outlives the user and the token, and the name
-- is a snapshot for the same reason. NULL on every answer the guest gave.
ALTER TABLE booking_answers ADD COLUMN attested_by_user_id TEXT;
ALTER TABLE booking_answers ADD COLUMN attested_by_name TEXT;
-- 'api_token' (a personal access token) or 'oauth' (an OAuth application).
ALTER TABLE booking_answers ADD COLUMN attested_via TEXT;
-- The personal access token's id, or the OAuth client's id.
ALTER TABLE booking_answers ADD COLUMN attested_credential_id TEXT;
