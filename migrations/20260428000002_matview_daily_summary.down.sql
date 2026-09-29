-- Down migration for 20260428000002_matview_daily_summary.sql
DROP INDEX IF EXISTS idx_events_daily_summary_unique;
DROP MATERIALIZED VIEW IF EXISTS events_daily_summary;
