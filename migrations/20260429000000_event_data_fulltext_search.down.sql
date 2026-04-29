-- Drop the GIN index
DROP INDEX IF EXISTS idx_events_event_data_tsv;
-- Drop the tsvector generated column
ALTER TABLE events DROP COLUMN IF EXISTS event_data_tsv;