-- Optimize ledger index for DESC sorts as per issue description
CREATE INDEX IF NOT EXISTS idx_events_ledger_desc ON events(ledger DESC);

-- Drop the old ascending index as all queries use ORDER BY ledger DESC
DROP INDEX IF EXISTS idx_events_ledger;
