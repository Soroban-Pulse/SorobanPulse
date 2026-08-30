-- Issue #932: Event time series analysis
-- Creates cache table for pre-computed time series buckets

CREATE TABLE IF NOT EXISTS event_time_series_cache (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    contract_id     TEXT,                          -- NULL means "all contracts"
    granularity     TEXT        NOT NULL,          -- 'hourly', 'daily', 'weekly', 'monthly'
    bucket_start    TIMESTAMPTZ NOT NULL,
    bucket_end      TIMESTAMPTZ NOT NULL,
    event_count     BIGINT      NOT NULL DEFAULT 0,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (contract_id, granularity, bucket_start)
);

-- Primary access pattern: look up a contract's buckets in time order
CREATE INDEX IF NOT EXISTS idx_ts_cache_contract_granularity_start
    ON event_time_series_cache(contract_id, granularity, bucket_start);

-- Range queries across all contracts for a given granularity
CREATE INDEX IF NOT EXISTS idx_ts_cache_granularity_start
    ON event_time_series_cache(granularity, bucket_start);

-- Allow efficient cache invalidation based on when data was computed
CREATE INDEX IF NOT EXISTS idx_ts_cache_computed_at
    ON event_time_series_cache(computed_at);
