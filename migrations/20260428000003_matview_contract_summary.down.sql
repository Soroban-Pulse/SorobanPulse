-- Down migration for 20260428000003_matview_contract_summary.sql
DROP INDEX IF EXISTS idx_events_contract_summary_unique;
DROP INDEX IF EXISTS idx_mv_contract_summary_unique;
DROP MATERIALIZED VIEW IF EXISTS events_contract_summary;
DROP MATERIALIZED VIEW IF EXISTS mv_contract_summary;
