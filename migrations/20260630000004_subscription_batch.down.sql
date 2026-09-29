-- Down migration for 20260630000004_subscription_batch.sql
DROP INDEX IF EXISTS idx_subscriptions_type;
ALTER TABLE subscriptions DROP COLUMN IF EXISTS subscription_type;
ALTER TABLE subscriptions DROP COLUMN IF EXISTS batch_size;
ALTER TABLE subscriptions DROP COLUMN IF EXISTS batch_timeout_ms;
