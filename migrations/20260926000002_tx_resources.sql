CREATE TABLE IF NOT EXISTS tx_resources (
    tx_hash       TEXT PRIMARY KEY,
    contract_id   TEXT NOT NULL,
    ledger        BIGINT NOT NULL,
    resource_fee  BIGINT NOT NULL DEFAULT 0,
    instructions  BIGINT NOT NULL DEFAULT 0,
    read_bytes    BIGINT NOT NULL DEFAULT 0,
    write_bytes   BIGINT NOT NULL DEFAULT 0,
    closed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_tx_resources_contract_time ON tx_resources (contract_id, closed_at);

CREATE MATERIALIZED VIEW IF NOT EXISTS mv_contract_resources_daily AS
SELECT contract_id,
       date_trunc('day', closed_at) AS day,
       COUNT(*) AS tx_count,
       percentile_cont(0.5)  WITHIN GROUP (ORDER BY resource_fee) AS fee_p50,
       percentile_cont(0.95) WITHIN GROUP (ORDER BY resource_fee) AS fee_p95,
       percentile_cont(0.5)  WITHIN GROUP (ORDER BY instructions) AS instructions_p50,
       percentile_cont(0.95) WITHIN GROUP (ORDER BY instructions) AS instructions_p95,
       percentile_cont(0.5)  WITHIN GROUP (ORDER BY read_bytes + write_bytes) AS io_bytes_p50,
       percentile_cont(0.95) WITHIN GROUP (ORDER BY read_bytes + write_bytes) AS io_bytes_p95
FROM tx_resources
GROUP BY contract_id, date_trunc('day', closed_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_contract_resources_daily ON mv_contract_resources_daily (contract_id, day);
