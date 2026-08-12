-- Default country calling code used to normalize local guest phone numbers.
ALTER TABLE twilio_config ADD COLUMN default_country_code TEXT NOT NULL DEFAULT '+1';
