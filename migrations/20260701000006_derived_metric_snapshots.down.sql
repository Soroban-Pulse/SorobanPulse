-- Reverts 20260701000006_derived_metric_snapshots.sql
DROP INDEX IF EXISTS idx_derived_metric_snapshots_labels;
DROP INDEX IF EXISTS idx_derived_metric_snapshots_name_evaluated_at;
DROP TABLE IF EXISTS derived_metric_snapshots;
