-- Down migration for 20260625000102_notification_audit_log.sql
DROP INDEX IF EXISTS idx_notification_audit_log_triggered_at;
DROP INDEX IF EXISTS idx_notification_audit_log_channel_type;
DROP INDEX IF EXISTS idx_notification_audit_log_status;
DROP TABLE IF EXISTS notification_audit_log CASCADE;
