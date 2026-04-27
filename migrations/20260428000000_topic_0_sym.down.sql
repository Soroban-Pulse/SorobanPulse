DROP INDEX IF EXISTS idx_events_topic_0_sym;
ALTER TABLE events DROP COLUMN IF EXISTS topic_0_sym;
