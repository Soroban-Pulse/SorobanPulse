-- Rollback: Restore the original ascending ledger index
CREATE INDEX IF NOT EXISTS idx_events_ledger ON events(ledger);

-- Drop the descending index
DROP INDEX IF EXISTS idx_events_ledger_desc;
