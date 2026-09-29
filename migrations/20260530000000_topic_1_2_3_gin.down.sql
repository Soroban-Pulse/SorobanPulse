-- Down migration for 20260530000000_topic_1_2_3_gin.sql
DROP INDEX IF EXISTS idx_events_topic_1_gin;
DROP INDEX IF EXISTS idx_events_topic_2_gin;
DROP INDEX IF EXISTS idx_events_topic_3_gin;
