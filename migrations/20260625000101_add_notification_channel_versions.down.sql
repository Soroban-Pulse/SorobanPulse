-- Down migration for 20260625000101_add_notification_channel_versions.sql
DROP INDEX IF EXISTS idx_ncv_channel_id;
DROP TABLE IF EXISTS notification_channel_versions CASCADE;
