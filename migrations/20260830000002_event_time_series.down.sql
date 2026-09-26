-- Down migration for 20260830000002_event_time_series.sql
DROP INDEX IF EXISTS idx_ts_cache_contract_granularity_start;
DROP INDEX IF EXISTS idx_ts_cache_granularity_start;
DROP INDEX IF EXISTS idx_ts_cache_computed_at;
DROP TABLE IF EXISTS event_time_series_cache CASCADE;
