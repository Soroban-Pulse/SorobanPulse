-- Composite index for contract_id filter and ledger DESC sort
CREATE INDEX IF NOT EXISTS idx_events_contract_ledger ON events(contract_id, ledger DESC);

-- Composite index for tx_hash filter and ledger DESC sort
CREATE INDEX IF NOT EXISTS idx_events_tx_ledger ON events(tx_hash, ledger DESC);

-- Drop redundant single-column indices
DROP INDEX IF EXISTS idx_events_contract_id;
DROP INDEX IF EXISTS idx_events_tx_hash;
