CREATE TABLE IF NOT EXISTS backfill_checkpoints (
    range_start BIGINT NOT NULL,
    range_end   BIGINT NOT NULL,
    next_ledger BIGINT NOT NULL,
    completed   BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (range_start, range_end)
);
