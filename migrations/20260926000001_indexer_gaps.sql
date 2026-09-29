CREATE TABLE IF NOT EXISTS indexer_gaps (
    id          BIGSERIAL PRIMARY KEY,
    from_ledger BIGINT NOT NULL,
    to_ledger   BIGINT NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (from_ledger, to_ledger)
);
