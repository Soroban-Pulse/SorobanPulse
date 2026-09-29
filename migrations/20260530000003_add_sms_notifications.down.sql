-- Down migration for 20260530000003_add_sms_notifications.sql
DROP INDEX IF EXISTS idx_sms_notifications_phone;
DROP INDEX IF EXISTS idx_sms_notifications_status;
DROP INDEX IF EXISTS idx_sms_notifications_created_at;
DROP TABLE IF EXISTS sms_notifications CASCADE;
