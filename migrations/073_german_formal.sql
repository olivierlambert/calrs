-- Address German-speaking visitors as "Sie" (1) rather than "du" (0, default).
-- Browsers only ask for "de"; this instance setting picks the register.
ALTER TABLE auth_config ADD COLUMN german_formal INTEGER NOT NULL DEFAULT 0;
