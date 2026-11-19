-- Reverts 20260701000005_webhook_dlq.sql
DROP INDEX IF EXISTS idx_webhook_dlq_replayed_at;
DROP INDEX IF EXISTS idx_webhook_dlq_failed_at;
DROP INDEX IF EXISTS idx_webhook_dlq_subscription_id;
DROP TABLE IF EXISTS webhook_dlq;
