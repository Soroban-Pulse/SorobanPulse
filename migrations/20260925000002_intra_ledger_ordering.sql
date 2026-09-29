-- Deterministic intra-ledger ordering with tx/op/event indexes (#1065).
--
-- RPC event ids encode a TOID (ledger, tx order, op index) plus an event
-- index. Persisting these gives a stable sort order within a ledger that
-- does not depend on insertion order, so re-indexing and replicas agree.
ALTER TABLE events ADD COLUMN IF NOT EXISTS tx_index INTEGER;
ALTER TABLE events ADD COLUMN IF NOT EXISTS op_index INTEGER;
ALTER TABLE events ADD COLUMN IF NOT EXISTS event_index INTEGER;

-- Existing rows have no ordinal information available after the fact; they
-- keep NULL and sort after any ordinal-tagged rows within the same ledger
-- (COALESCE to a high placeholder in queries), preserving today's behaviour
-- for historical data while all newly-indexed events populate all three.
CREATE INDEX IF NOT EXISTS idx_events_ledger_ordinal
    ON events (ledger, tx_index, op_index, event_index);
