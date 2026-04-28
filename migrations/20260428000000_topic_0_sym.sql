ALTER TABLE events
    ADD COLUMN IF NOT EXISTS topic_0_sym TEXT
        GENERATED ALWAYS AS (event_data->'topic'->0->>'sym') STORED;

CREATE INDEX IF NOT EXISTS idx_events_topic_0_sym ON events(topic_0_sym);
