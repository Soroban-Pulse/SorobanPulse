CREATE TABLE IF NOT EXISTS event_addresses (
    event_id UUID NOT NULL,
    address TEXT NOT NULL,
    position SMALLINT NOT NULL,
    ledger BIGINT NOT NULL,
    PRIMARY KEY (event_id, address, position)
);
-- Lookup is by address ordered by ledger desc; (address, ledger DESC, event_id DESC)
-- serves both the filter and the keyset pagination without a sort.
CREATE INDEX IF NOT EXISTS idx_event_addresses_lookup ON event_addresses(address, ledger DESC, event_id DESC);
