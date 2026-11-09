-- landing_sections_and_profile_facts
-- The landing pages (/u/{username}, /team/{slug}) group meetings into
-- host-written sections and list them in the host's order, and show where
-- the host is based and the languages they speak.
--
-- event_types.section: free text, shown as written; NULL or empty = no
-- section. event_types.page_position: the type's place on its page (the
-- personal page of its owner, or its team's page); sections follow the
-- position of their first type. NULL = after every placed type, in
-- creation order, so existing pages keep their order.
ALTER TABLE event_types ADD COLUMN section TEXT;
ALTER TABLE event_types ADD COLUMN page_position INTEGER;

-- users.based_in: free text (a city or region). users.spoken_languages:
-- BCP-47 codes joined with commas ("fr,en"), shown as language names in
-- the visitor's language. NULL = not shown.
ALTER TABLE users ADD COLUMN based_in TEXT;
ALTER TABLE users ADD COLUMN spoken_languages TEXT;
