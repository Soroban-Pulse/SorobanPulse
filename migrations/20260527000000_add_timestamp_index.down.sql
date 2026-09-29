-- Down migration for 20260527000000_add_timestamp_index.sql
DROP INDEX IF EXISTS idx_events_timestamp_id;
