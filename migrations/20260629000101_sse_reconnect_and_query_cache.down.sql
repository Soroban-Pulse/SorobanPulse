-- Down migration for 20260629000101_sse_reconnect_and_query_cache.sql
DROP INDEX IF EXISTS mv_contract_event_counts_uniq;
DROP INDEX IF EXISTS mv_contract_event_counts_contract_idx;
DROP INDEX IF EXISTS events_daily_summary_uniq;
DROP MATERIALIZED VIEW IF EXISTS mv_contract_event_counts;
DROP MATERIALIZED VIEW IF EXISTS events_daily_summary;
