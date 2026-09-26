-- Down migration for 20260530000002_add_notification_formats.sql
DROP INDEX IF EXISTS idx_webhook_configs_active;
DROP INDEX IF EXISTS idx_webhook_configs_format;
DROP INDEX IF EXISTS idx_pagerduty_configs_active;
DROP INDEX IF EXISTS idx_pagerduty_incidents_status;
DROP INDEX IF EXISTS idx_pagerduty_incidents_contract;
DROP INDEX IF EXISTS idx_pagerduty_incidents_dedup_key;
DROP TABLE IF EXISTS webhook_configs CASCADE;
DROP TABLE IF EXISTS pagerduty_configs CASCADE;
DROP TABLE IF EXISTS pagerduty_incidents CASCADE;
