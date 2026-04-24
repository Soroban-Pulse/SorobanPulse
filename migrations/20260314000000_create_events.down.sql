-- Rollback: Drop the events table and all associated indices
DROP INDEX IF EXISTS idx_events_tx_hash_contract;
DROP INDEX IF EXISTS idx_events_ledger;
DROP INDEX IF EXISTS idx_events_tx_hash;
DROP INDEX IF EXISTS idx_events_contract_id;
DROP TABLE IF EXISTS events;
