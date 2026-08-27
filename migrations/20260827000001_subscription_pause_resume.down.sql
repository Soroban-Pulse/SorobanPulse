-- Rollback issue #884: subscription pause/resume

DROP INDEX IF EXISTS idx_subscription_pause_resume_log_action;
DROP INDEX IF EXISTS idx_subscription_pause_resume_log_subscription;
DROP INDEX IF EXISTS idx_subscriptions_pause_until;

DROP TABLE IF EXISTS subscription_pause_resume_log;

ALTER TABLE subscriptions DROP COLUMN IF EXISTS created_by;
ALTER TABLE subscriptions DROP COLUMN IF EXISTS paused_at;
ALTER TABLE subscriptions DROP COLUMN IF EXISTS pause_reason;
ALTER TABLE subscriptions DROP COLUMN IF EXISTS pause_until;
