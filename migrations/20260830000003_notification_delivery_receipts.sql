-- Issue #933: Enhanced notification delivery receipts.
--
-- Extends the notification_deliveries table with channel metadata,
-- retry tracking, latency measurement, and adds a stats view for
-- dashboard use.

-- Add extended receipt tracking columns
ALTER TABLE notification_deliveries
    ADD COLUMN IF NOT EXISTS channel_metadata JSONB;

ALTER TABLE notification_deliveries
    ADD COLUMN IF NOT EXISTS retry_count INT NOT NULL DEFAULT 0;

ALTER TABLE notification_deliveries
    ADD COLUMN IF NOT EXISTS latency_ms INT;

-- Index supporting retention policy queries (purge oldest receipts first)
CREATE INDEX IF NOT EXISTS idx_notification_deliveries_retention
    ON notification_deliveries (delivered_at);

-- Composite index for per-channel dashboard queries
CREATE INDEX IF NOT EXISTS idx_notification_deliveries_channel
    ON notification_deliveries (channel_type, status, delivered_at DESC);

-- Aggregated stats view used by get_receipt_stats()
CREATE OR REPLACE VIEW notification_delivery_stats AS
SELECT
    channel_type,
    status,
    COUNT(*)            AS count,
    AVG(latency_ms)     AS avg_latency_ms,
    MAX(delivered_at)   AS last_delivery
FROM notification_deliveries
GROUP BY channel_type, status;
