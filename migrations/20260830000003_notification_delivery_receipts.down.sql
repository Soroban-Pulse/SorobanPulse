-- Down migration for 20260830000003_notification_delivery_receipts.sql
DROP INDEX IF EXISTS idx_notification_deliveries_retention;
DROP INDEX IF EXISTS idx_notification_deliveries_channel;
DROP VIEW IF EXISTS notification_delivery_stats;
ALTER TABLE notification_deliveries DROP COLUMN IF EXISTS channel_metadata;
ALTER TABLE notification_deliveries DROP COLUMN IF EXISTS retry_count;
ALTER TABLE notification_deliveries DROP COLUMN IF EXISTS latency_ms;
