-- Down migration for 20260530000101_add_notification_channels.sql
DROP INDEX IF EXISTS idx_email_notifications_idempotency;
DROP INDEX IF EXISTS idx_email_notifications_sent_at;
DROP TABLE IF EXISTS notification_channels CASCADE;
DROP TABLE IF EXISTS email_notifications CASCADE;
