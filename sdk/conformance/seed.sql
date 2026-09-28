-- sdk/conformance/seed.sql
--
-- Conformance seed data for SDK conformance tests.
-- Must be applied to the database before any SDK runner executes.
--
-- Constants guaranteed by this file (referenced in scenarios.yaml):
--   CONTRACT_A  = CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4
--   CONTRACT_B  = CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBFCT4
--   TX_HASH_1   = 0000000000000000000000000000000000000000000000000000000000000001
--   TX_HASH_UNKNOWN = ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
--
-- Total row guarantee: at least 65 events so list_events_no_filter (total >= 65) passes.
--   CONTRACT_A: 50 contract-type events   (ledgers 1001-1050)
--   CONTRACT_B: 10 diagnostic-type events (ledgers 1001-1010)
--   CONTRACT_C:  5 system-type events     (ledgers 1001-1005)
--   ──────────────────────────────────────
--   Total:      65 events minimum
--
-- TX_HASH_1 is assigned to CONTRACT_A event i=1 (ledger 1001) so that
-- scenario get_events_by_tx_known returns at least one row.

-- ── CONTRACT_A: 50 contract events ───────────────────────────────────────────
INSERT INTO events (
    id,
    contract_id,
    event_type,
    tx_hash,
    ledger,
    ledger_closed_at,
    event_data,
    created_at
)
SELECT
    gen_random_uuid(),
    'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4',
    'contract',
    -- Event 1 uses TX_HASH_1 so get_events_by_tx_known passes
    CASE WHEN i = 1
         THEN '0000000000000000000000000000000000000000000000000000000000000001'
         ELSE lpad(i::text, 64, '0')
    END,
    1000 + i,
    (TIMESTAMP '2026-03-14 00:00:00' + (i || ' minutes')::interval),
    jsonb_build_object(
        'value',  jsonb_build_object('i128', jsonb_build_object('hi', 0, 'lo', i * 100)),
        'topic',  jsonb_build_array('transfer', 'GADDR' || lpad(i::text, 8, '0'))
    ),
    NOW()
FROM generate_series(1, 50) AS i
ON CONFLICT DO NOTHING;

-- ── CONTRACT_B: 10 diagnostic events ─────────────────────────────────────────
INSERT INTO events (
    id,
    contract_id,
    event_type,
    tx_hash,
    ledger,
    ledger_closed_at,
    event_data,
    created_at
)
SELECT
    gen_random_uuid(),
    'CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBFCT4',
    'diagnostic',
    lpad((100 + i)::text, 64, '0'),
    1000 + i,
    (TIMESTAMP '2026-03-14 00:00:00' + (i || ' minutes')::interval),
    jsonb_build_object(
        'value',  jsonb_build_object('string', 'diagnostic-event-' || i),
        'topic',  jsonb_build_array('log')
    ),
    NOW()
FROM generate_series(1, 10) AS i
ON CONFLICT DO NOTHING;

-- ── CONTRACT_C: 5 system events ───────────────────────────────────────────────
INSERT INTO events (
    id,
    contract_id,
    event_type,
    tx_hash,
    ledger,
    ledger_closed_at,
    event_data,
    created_at
)
SELECT
    gen_random_uuid(),
    'CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCFCT4',
    'system',
    lpad((200 + i)::text, 64, '0'),
    1000 + i,
    (TIMESTAMP '2026-03-14 00:00:00' + (i || ' minutes')::interval),
    jsonb_build_object(
        'value',  jsonb_build_object('string', 'system-event-' || i),
        'topic',  jsonb_build_array('fee')
    ),
    NOW()
FROM generate_series(1, 5) AS i
ON CONFLICT DO NOTHING;
