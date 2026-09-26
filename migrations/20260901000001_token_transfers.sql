CREATE TABLE IF NOT EXISTS token_transfers (
    event_id UUID PRIMARY KEY,
    contract_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    from_address TEXT,
    to_address TEXT,
    spender TEXT,
    amount NUMERIC(40, 0),
    ledger BIGINT NOT NULL,
    tx_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_token_transfers_contract ON token_transfers(contract_id, ledger DESC, event_id DESC);
CREATE INDEX IF NOT EXISTS idx_token_transfers_from ON token_transfers(from_address);
CREATE INDEX IF NOT EXISTS idx_token_transfers_to ON token_transfers(to_address);
CREATE INDEX IF NOT EXISTS idx_token_transfers_ledger ON token_transfers(ledger);
