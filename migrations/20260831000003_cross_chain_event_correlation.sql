-- Issue #935: Add cross-chain event correlation

-- Network identifier on the event model itself, so every indexed event
-- records which chain/network it came from.
ALTER TABLE events ADD COLUMN IF NOT EXISTS network TEXT NOT NULL DEFAULT 'soroban-mainnet';

CREATE INDEX IF NOT EXISTS idx_events_network ON events(network);
CREATE INDEX IF NOT EXISTS idx_events_network_contract ON events(network, contract_id);

-- Network-specific deduplication: a fingerprint is only a duplicate within
-- the same network, not across networks (Issue #935: "Implement
-- network-specific deduplication"). Replaces the network-agnostic
-- uniqueness assumption implicit in the existing fingerprint column.
CREATE INDEX IF NOT EXISTS idx_events_network_fingerprint
    ON events(network, fingerprint) WHERE fingerprint IS NOT NULL;

-- Persisted cross-chain correlations between two events (mirrors
-- cross_chain_correlation::EventCorrelation).
CREATE TABLE IF NOT EXISTS cross_chain_correlations (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    source_event_id   UUID        NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    target_event_id   UUID        NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    source_network    TEXT        NOT NULL,
    target_network    TEXT        NOT NULL,
    causality         TEXT        NOT NULL, -- Direct | Indirect | Related | Sequential
    confidence        DOUBLE PRECISION NOT NULL,
    reason            TEXT        NOT NULL,
    detected_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (source_event_id, target_event_id)
);

CREATE INDEX IF NOT EXISTS idx_cross_chain_correlations_source
    ON cross_chain_correlations(source_event_id);
CREATE INDEX IF NOT EXISTS idx_cross_chain_correlations_target
    ON cross_chain_correlations(target_event_id);
CREATE INDEX IF NOT EXISTS idx_cross_chain_correlations_networks
    ON cross_chain_correlations(source_network, target_network);

-- Named groupings of events that form one cross-chain flow (Issue #935:
-- "Add cross-chain event grouping"), independent of the pairwise
-- correlation edges above.
CREATE TABLE IF NOT EXISTS cross_chain_event_groups (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    label        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS cross_chain_event_group_members (
    group_id   UUID NOT NULL REFERENCES cross_chain_event_groups(id) ON DELETE CASCADE,
    event_id   UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, event_id)
);

CREATE INDEX IF NOT EXISTS idx_cross_chain_event_group_members_event
    ON cross_chain_event_group_members(event_id);
