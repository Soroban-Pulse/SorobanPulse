-- Down migration for 20260625000100_notification_failover_and_costs.sql
DROP INDEX IF EXISTS idx_notification_costs_channel_id;
DROP INDEX IF EXISTS idx_notification_costs_sent_at;
DROP TABLE IF EXISTS notification_costs CASCADE;
ALTER TABLE notification_channels DROP COLUMN IF EXISTS failover_channel_id;
ALTER TABLE notification_channels DROP COLUMN IF EXISTS cost_per_notification_cents;
