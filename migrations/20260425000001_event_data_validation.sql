-- Add CHECK constraint to enforce event_data structure
-- Ensures value is an object or null, and topic is an array or null
ALTER TABLE events
ADD CONSTRAINT check_event_data_structure CHECK (
    (event_data->'value' IS NULL OR jsonb_typeof(event_data->'value') = 'object') AND
    (event_data->'topic' IS NULL OR jsonb_typeof(event_data->'topic') = 'array')
);
