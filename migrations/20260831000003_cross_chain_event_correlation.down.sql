-- Issue #935: rollback cross-chain event correlation

DROP TABLE IF EXISTS cross_chain_event_group_members;
DROP TABLE IF EXISTS cross_chain_event_groups;
DROP TABLE IF EXISTS cross_chain_correlations;
DROP INDEX IF EXISTS idx_events_network_fingerprint;
DROP INDEX IF EXISTS idx_events_network_contract;
DROP INDEX IF EXISTS idx_events_network;
ALTER TABLE events DROP COLUMN IF EXISTS network;
