DROP INDEX IF EXISTS idx_events_ledger_ordinal;
ALTER TABLE events DROP COLUMN IF EXISTS event_index;
ALTER TABLE events DROP COLUMN IF EXISTS op_index;
ALTER TABLE events DROP COLUMN IF EXISTS tx_index;
