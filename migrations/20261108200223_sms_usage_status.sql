-- sms_usage_status (#399): what became of each metered SMS.
--
-- A row is inserted as 'pending' before the gateway is called (the cap's
-- reservation), then becomes 'sent' when the gateway accepted the message,
-- or 'unknown' when the request failed in transit (timeout, connection
-- reset, a 5xx) and the gateway may have sent and billed it anyway. Every
-- status counts against the daily cap: a spend cap errs toward
-- over-counting. A definite refusal deletes the row instead. Rows written
-- before this migration were all accepted messages.
ALTER TABLE sms_usage ADD COLUMN status TEXT NOT NULL DEFAULT 'sent';
