CREATE TABLE IF NOT EXISTS indexer_checkpoints (
    id          TEXT PRIMARY KEY DEFAULT 'singleton',
    last_cursor TEXT        NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
