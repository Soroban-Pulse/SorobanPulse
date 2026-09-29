-- Down migration for 20260625000001_add_notification_acknowledgments.sql
DROP INDEX IF EXISTS idx_notif_ack_status_created;
DROP INDEX IF EXISTS idx_notif_ack_channel;
DROP TABLE IF EXISTS notification_acknowledgments CASCADE;
