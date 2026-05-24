-- Per-row TLS mode for the SMTP transport. NULL/'starttls' (default) means
-- the existing STARTTLS handshake; 'tls' means implicit TLS (typically port
-- 465), which previously caused send_email() to hang because the code path
-- unconditionally used starttls_relay().
ALTER TABLE smtp_config ADD COLUMN tls_mode TEXT NOT NULL DEFAULT 'starttls';
