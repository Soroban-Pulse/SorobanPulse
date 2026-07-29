-- Issue #818: Query Optimization & Execution Plan Analysis
--
-- slow_query_log: persisted slow query records for trend analysis.
-- query_explain_cache: stores EXPLAIN output for repeated queries.

CREATE TABLE IF NOT EXISTS slow_query_log (
    id                  BIGSERIAL PRIMARY KEY,
    query_fingerprint   TEXT        NOT NULL,
    sample_query        TEXT        NOT NULL,
    avg_duration_ms     DOUBLE PRECISION NOT NULL,
    max_duration_ms     DOUBLE PRECISION NOT NULL,
    call_count          BIGINT      NOT NULL DEFAULT 1,
    first_seen_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_slow_query_log_fingerprint
    ON slow_query_log (query_fingerprint);

CREATE INDEX IF NOT EXISTS idx_slow_query_log_avg_duration
    ON slow_query_log (avg_duration_ms DESC);

CREATE TABLE IF NOT EXISTS query_explain_cache (
    id                  BIGSERIAL PRIMARY KEY,
    query_fingerprint   TEXT        NOT NULL UNIQUE,
    sample_query        TEXT        NOT NULL,
    explain_output      JSONB       NOT NULL,
    total_cost          DOUBLE PRECISION,
    seq_scans           TEXT[],
    index_scans         TEXT[],
    warnings            TEXT[],
    recommendations     TEXT[],
    cached_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at          TIMESTAMPTZ NOT NULL DEFAULT now() + INTERVAL '1 hour'
);

CREATE INDEX IF NOT EXISTS idx_query_explain_cache_expires
    ON query_explain_cache (expires_at);
