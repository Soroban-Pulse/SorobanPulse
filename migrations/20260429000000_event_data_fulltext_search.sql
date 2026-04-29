-- Add tsvector generated column for full-text search on event_data
ALTER TABLE events
ADD COLUMN event_data_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', event_data::text)) STORED;
-- Create GIN index for efficient full-text search
CREATE INDEX idx_events_event_data_tsv ON events USING GIN (event_data_tsv);