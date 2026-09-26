-- Down migration for 20260830000101_event_grouping_aggregation.sql
DROP INDEX IF EXISTS idx_group_metrics_rule_window;
DROP INDEX IF EXISTS idx_group_metrics_subscription;
DROP INDEX IF EXISTS idx_group_metrics_group_key;
DROP INDEX IF EXISTS idx_group_metrics_rule_group_time;
DROP TABLE IF EXISTS group_metrics CASCADE;
ALTER TABLE aggregation_rules DROP COLUMN IF EXISTS aggregation_ops;
ALTER TABLE aggregation_rules DROP COLUMN IF EXISTS batch_size;
