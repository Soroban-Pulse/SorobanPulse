-- Down migration for 20260530000004_add_event_aggregation.sql
DROP INDEX IF EXISTS idx_aggregation_rules_name;
DROP INDEX IF EXISTS idx_aggregation_rules_created_at;
DROP TABLE IF EXISTS aggregation_rules CASCADE;
