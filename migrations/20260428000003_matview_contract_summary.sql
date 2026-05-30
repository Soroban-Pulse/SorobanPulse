CREATE MATERIALIZED VIEW IF NOT EXISTS events_contract_summary AS
SELECT
    contract_id,
    COUNT(*)     AS event_count,
    MAX(ledger)  AS latest_ledger
FROM events
GROUP BY contract_id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_events_contract_summary_unique
    ON events_contract_summary (contract_id);

CREATE MATERIALIZED VIEW IF NOT EXISTS mv_contract_summary AS
SELECT
    contract_id,
    timestamp::date AS event_date,
    COUNT(*) AS event_count,
    COUNT(DISTINCT tx_hash) AS unique_tx_count
FROM events
GROUP BY contract_id, timestamp::date;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_contract_summary_unique
    ON mv_contract_summary (contract_id, event_date);
