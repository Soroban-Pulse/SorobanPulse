-- Down migration for 20260424000000_gin_index_event_data.sql
DROP INDEX IF EXISTS idx_events_event_data_gin;
