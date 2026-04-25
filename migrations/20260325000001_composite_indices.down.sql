-- Rollback: Restore the original single-column indices
CREATE INDEX IF NOT EXISTS idx_events_contract_id ON events(contract_id);
CREATE INDEX IF NOT EXISTS idx_events_tx_hash ON events(tx_hash);

-- Drop the composite indices
DROP INDEX IF EXISTS idx_events_tx_ledger;
DROP INDEX IF EXISTS idx_events_contract_ledger;
