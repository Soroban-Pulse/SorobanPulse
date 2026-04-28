CREATE MATERIALIZED VIEW IF NOT EXISTS events_hourly_volume AS
SELECT
    DATE_TRUNC('hour', timestamp) AS event_hour,
    COUNT(*)                      AS event_count
FROM events
WHERE timestamp >= NOW() - INTERVAL '7 days'
GROUP BY DATE_TRUNC('hour', timestamp);

CREATE UNIQUE INDEX IF NOT EXISTS idx_events_hourly_volume_unique
    ON events_hourly_volume (event_hour);
