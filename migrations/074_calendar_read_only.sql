-- Calendars the account can read but not write (#229).
--
-- Microsoft Graph lists a calendar a colleague shared, once accepted in
-- Outlook, among the user's own calendars, reports it as `canEdit: false`
-- and refuses writes to it with RemoteCalendarAccessDenied. Such a calendar
-- is not offered as a write-back destination. Providers that do not report
-- write access leave it 0.
ALTER TABLE calendars ADD COLUMN read_only INTEGER NOT NULL DEFAULT 0;
