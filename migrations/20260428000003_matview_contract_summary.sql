CREATE MATERIALIZED VIEW IF NOT EXISTS events_contract_summary AS
SELECT
    contract_id,
    COUNT(*)     AS event_count,
    MAX(ledger)  AS latest_ledger
FROM events
GROUP BY contract_id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_events_contract_summary_unique
    ON events_contract_summary (contract_id);
