CREATE MATERIALIZED VIEW IF NOT EXISTS events_daily_summary AS
SELECT
    DATE(timestamp)  AS event_date,
    event_type,
    COUNT(*)         AS event_count
FROM events
GROUP BY DATE(timestamp), event_type;

CREATE UNIQUE INDEX IF NOT EXISTS idx_events_daily_summary_unique
    ON events_daily_summary (event_date, event_type);
