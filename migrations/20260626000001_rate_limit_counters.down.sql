-- Down migration for 20260626000001_rate_limit_counters.sql
DROP INDEX IF EXISTS idx_rate_limit_api_key_hash;
DROP INDEX IF EXISTS idx_rate_limit_window_start;
DROP INDEX IF EXISTS idx_rate_limit_active_windows;
DROP FUNCTION IF EXISTS cleanup_old_rate_limit_windows() CASCADE;
DROP TABLE IF EXISTS rate_limit_counters CASCADE;
