-- Down migration for 20260527000003_gin_index_event_data_topic.sql
DROP INDEX IF EXISTS idx_events_event_data_topic_gin;
