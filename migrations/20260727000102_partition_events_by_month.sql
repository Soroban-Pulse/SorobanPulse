-- Issue #691: Implement table partitioning by time
-- This migration sets up monthly range partitioning on the events table
-- to improve query performance on large datasets

-- Step 1: Create the partitioned events table
CREATE TABLE IF NOT EXISTS events_partitioned (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    contract_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    tx_hash TEXT NOT NULL,
    ledger BIGINT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    event_data JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
) PARTITION BY RANGE (timestamp);

-- Step 2: Create partitions for the last 12 months and next 3 months
-- This ensures we have partitions ready for current and near-future data

CREATE TABLE IF NOT EXISTS events_2025_07 PARTITION OF events_partitioned
    FOR VALUES FROM ('2025-07-01') TO ('2025-08-01');

CREATE TABLE IF NOT EXISTS events_2025_08 PARTITION OF events_partitioned
    FOR VALUES FROM ('2025-08-01') TO ('2025-09-01');

CREATE TABLE IF NOT EXISTS events_2025_09 PARTITION OF events_partitioned
    FOR VALUES FROM ('2025-09-01') TO ('2025-10-01');

CREATE TABLE IF NOT EXISTS events_2025_10 PARTITION OF events_partitioned
    FOR VALUES FROM ('2025-10-01') TO ('2025-11-01');

CREATE TABLE IF NOT EXISTS events_2025_11 PARTITION OF events_partitioned
    FOR VALUES FROM ('2025-11-01') TO ('2025-12-01');

CREATE TABLE IF NOT EXISTS events_2025_12 PARTITION OF events_partitioned
    FOR VALUES FROM ('2025-12-01') TO ('2026-01-01');

CREATE TABLE IF NOT EXISTS events_2026_01 PARTITION OF events_partitioned
    FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');

CREATE TABLE IF NOT EXISTS events_2026_02 PARTITION OF events_partitioned
    FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');

CREATE TABLE IF NOT EXISTS events_2026_03 PARTITION OF events_partitioned
    FOR VALUES FROM ('2026-03-01') TO ('2026-04-01');

CREATE TABLE IF NOT EXISTS events_2026_04 PARTITION OF events_partitioned
    FOR VALUES FROM ('2026-04-01') TO ('2026-05-01');

CREATE TABLE IF NOT EXISTS events_2026_05 PARTITION OF events_partitioned
    FOR VALUES FROM ('2026-05-01') TO ('2026-06-01');

CREATE TABLE IF NOT EXISTS events_2026_06 PARTITION OF events_partitioned
    FOR VALUES FROM ('2026-06-01') TO ('2026-07-01');

CREATE TABLE IF NOT EXISTS events_2026_07 PARTITION OF events_partitioned
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');

CREATE TABLE IF NOT EXISTS events_2026_08 PARTITION OF events_partitioned
    FOR VALUES FROM ('2026-08-01') TO ('2026-09-01');

CREATE TABLE IF NOT EXISTS events_2026_09 PARTITION OF events_partitioned
    FOR VALUES FROM ('2026-09-01') TO ('2026-10-01');

-- Step 3: Create indexes on all partitions for efficient querying
-- These indexes will be inherited by child partitions automatically

CREATE INDEX IF NOT EXISTS idx_events_partitioned_contract_id
    ON events_partitioned(contract_id);

CREATE INDEX IF NOT EXISTS idx_events_partitioned_tx_hash
    ON events_partitioned(tx_hash);

CREATE INDEX IF NOT EXISTS idx_events_partitioned_ledger
    ON events_partitioned(ledger);

CREATE UNIQUE INDEX IF NOT EXISTS idx_events_partitioned_tx_hash_contract
    ON events_partitioned(tx_hash, contract_id, event_type);

CREATE INDEX IF NOT EXISTS idx_events_partitioned_contract_ledger
    ON events_partitioned(contract_id, ledger DESC);

CREATE INDEX IF NOT EXISTS idx_events_partitioned_timestamp
    ON events_partitioned(timestamp DESC);

-- Step 4: Copy data from existing events table to partitioned table
-- This is done with INSERT INTO ... SELECT to preserve data integrity
INSERT INTO events_partitioned (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, created_at)
SELECT id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, created_at
FROM events
ON CONFLICT DO NOTHING;

-- Step 5: Create a view to maintain backward compatibility
-- Applications can continue using the events table name
CREATE OR REPLACE VIEW events_view AS
SELECT * FROM events_partitioned;

-- Step 6: Rename tables to maintain compatibility
-- Keep events_partitioned as the main table
-- Create a reference for legacy access
ALTER TABLE events RENAME TO events_legacy;
ALTER TABLE events_partitioned RENAME TO events;

-- Step 7: Recreate materialized view to use partitioned table
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
