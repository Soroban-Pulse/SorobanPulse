-- no-transaction
-- Reversal of 20260728000001_index_consolidation.sql
-- Recreates the two indexes that were dropped during consolidation.

-- 1. Recreate idx_events_contract_ledger
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_contract_ledger
    ON events(contract_id, ledger DESC);

-- 2. Recreate idx_events_event_data_topic_gin
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_event_data_topic_gin
    ON events USING GIN (event_data -> 'topic');
