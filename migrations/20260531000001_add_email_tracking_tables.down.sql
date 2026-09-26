-- Down migration for 20260531000001_add_email_tracking_tables.sql
DROP INDEX IF EXISTS idx_email_opens_token;
DROP INDEX IF EXISTS idx_email_opens_recipient;
DROP INDEX IF EXISTS idx_email_opens_opened_at;
DROP INDEX IF EXISTS idx_email_clicks_token;
DROP INDEX IF EXISTS idx_email_clicks_recipient;
DROP INDEX IF EXISTS idx_email_clicks_clicked_at;
DROP TABLE IF EXISTS email_opens CASCADE;
DROP TABLE IF EXISTS email_clicks CASCADE;
