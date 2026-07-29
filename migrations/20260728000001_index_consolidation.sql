-- no-transaction
-- Migration: 20260728000001_index_consolidation
-- Issue #804 — Database Schema Consolidation
--
-- Drops two redundant indexes identified in docs/schema-audit.md:
--
--   1. idx_events_contract_ledger  — superseded by the three-column
--      idx_events_contract_type_ledger which covers every query that
--      previously hit the two-column version.
--
--   2. idx_events_event_data_topic_gin — its containment queries are fully
--      covered by idx_events_event_data_gin (full-document jsonb_path_ops)
--      and idx_events_topic_1/2/3_gin (per-position jsonb_path_ops).
--
-- All DROPs use CONCURRENTLY so the operation is safe on a live database
-- and does not acquire an AccessExclusiveLock on the table.
-- The "-- no-transaction" header is required by SQLx when CONCURRENTLY is used.

-- 1. Drop idx_events_contract_ledger
--    Superseded by idx_events_contract_type_ledger (contract_id, event_type, ledger DESC).
--    The three-column index satisfies (contract_id, ledger DESC) queries via leading-column
--    scan; retaining both wastes ~8 bytes per row in write overhead.
DROP INDEX CONCURRENTLY IF EXISTS idx_events_contract_ledger;

-- 2. Drop idx_events_event_data_topic_gin
--    Uses the default jsonb_ops operator class on (event_data -> 'topic').
--    All containment (@>) queries on the topic array are already served by
--    idx_events_event_data_gin (jsonb_path_ops, full document) which is smaller
--    and faster, and by idx_events_topic_1/2/3_gin for per-position lookups.
DROP INDEX CONCURRENTLY IF EXISTS idx_events_event_data_topic_gin;

-- Add descriptive COMMENTs on the surviving indexes so future contributors
-- understand what each index is for without reading the migration history.
COMMENT ON INDEX idx_events_ledger_desc              IS 'Global paginated list: GET /v1/events ORDER BY ledger DESC';
COMMENT ON INDEX idx_events_tx_ledger                IS 'Tx-hash lookup: GET /v1/events/tx/{tx_hash} ORDER BY ledger DESC';
COMMENT ON INDEX idx_events_tx_hash_contract         IS 'Deduplication constraint + ON CONFLICT DO NOTHING target';
COMMENT ON INDEX idx_events_contract_type_ledger     IS 'Contract filter with optional event_type: GET /v1/events/{contract_id}';
COMMENT ON INDEX idx_events_type_ledger              IS 'Global event_type filter: GET /v1/events?event_type=contract';
COMMENT ON INDEX idx_events_contract_type_partial    IS 'Hot-path partial index for contract events (event_type = contract) ordered by ledger';
COMMENT ON INDEX idx_events_created_at_id            IS 'SSE cursor pagination: ORDER BY created_at DESC, id DESC';
COMMENT ON INDEX idx_events_event_data_gin           IS 'JSONB containment queries on full event_data document (@> operator)';
COMMENT ON INDEX idx_events_timestamp                IS 'Timestamp range scans; also used as partition key reference';
