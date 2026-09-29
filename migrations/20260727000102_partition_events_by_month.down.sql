-- Rollback: Issue #691 - Implement table partitioning by time
-- This migration reverses the partitioning setup

-- Step 1: Drop the partitioned table and all partitions
DROP TABLE IF EXISTS events CASCADE;

-- Step 2: Rename the legacy table back to events
ALTER TABLE events_legacy RENAME TO events;

-- Step 3: Recreate the original indexes
CREATE INDEX IF NOT EXISTS idx_events_contract_id ON events(contract_id);
CREATE INDEX IF NOT EXISTS idx_events_tx_hash ON events(tx_hash);
CREATE INDEX IF NOT EXISTS idx_events_ledger ON events(ledger);
CREATE UNIQUE INDEX IF NOT EXISTS idx_events_tx_hash_contract ON events(tx_hash, contract_id, event_type);
CREATE INDEX IF NOT EXISTS idx_events_contract_ledger ON events(contract_id, ledger DESC);

-- Step 4: Recreate the materialized view
DROP MATERIALIZED VIEW IF EXISTS mv_contract_summary CASCADE;

CREATE MATERIALIZED VIEW mv_contract_summary AS
SELECT
    contract_id,
    COUNT(*)                                                        AS event_count,
    MAX(timestamp)                                                  AS latest_event_timestamp,
    MIN(timestamp)                                                  AS first_event_timestamp,
    MIN(ledger)                                                     AS min_ledger,
    MAX(ledger)                                                     AS max_ledger,
    COUNT(DISTINCT tx_hash)                                         AS unique_tx_count,
    COUNT(*) FILTER (WHERE event_type = 'contract')                 AS contract_events,
    COUNT(*) FILTER (WHERE event_type = 'diagnostic')               AS diagnostic_events,
    COUNT(*) FILTER (WHERE event_type = 'system')                   AS system_events
FROM events
GROUP BY contract_id;

CREATE UNIQUE INDEX idx_mv_contract_summary_contract_id
    ON mv_contract_summary (contract_id);

CREATE INDEX idx_mv_contract_summary_event_count
    ON mv_contract_summary (event_count DESC);

CREATE INDEX idx_mv_contract_summary_latest_event_timestamp
    ON mv_contract_summary (latest_event_timestamp DESC);

CREATE INDEX idx_mv_contract_summary_first_event_timestamp
    ON mv_contract_summary (first_event_timestamp ASC);
