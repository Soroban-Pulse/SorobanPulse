-- Rollback Issue #678: Webhook templating system

DROP INDEX IF EXISTS idx_template_usage_stats_template;
DROP INDEX IF EXISTS idx_template_validation_log_template;
DROP INDEX IF EXISTS idx_webhook_templates_subscription;

DROP TABLE IF EXISTS template_usage_stats;
DROP TABLE IF EXISTS template_validation_log;
DROP TABLE IF EXISTS webhook_templates;

ALTER TABLE subscriptions
    DROP COLUMN IF EXISTS webhook_template_enabled,
    DROP COLUMN IF EXISTS webhook_template;
