# Schema Audit — SorobanPulse (#804)

**Date:** 2026-07-28  
**Scope:** All migration files in `migrations/`, current index strategy, GIN index overlap, partition strategy.

---

## 1. Migration Inventory

| File | Object type | Object name | Action | Status |
|------|------------|-------------|--------|--------|
| `20260314000000_create_events` | Table | `events` | CREATE | **Active** |
| `20260314000000_create_events` | Index | `idx_events_contract_id` | CREATE | **Superseded** — dropped in `20260325000001` |
| `20260314000000_create_events` | Index | `idx_events_tx_hash` | CREATE | **Superseded** — dropped in `20260325000001` |
| `20260314000000_create_events` | Index | `idx_events_ledger` | CREATE | **Superseded** — dropped in `20260325000000` |
| `20260314000000_create_events` | Unique index | `idx_events_tx_hash_contract` | CREATE | **Active** |
| `20260325000000_optimize_ledger_index` | Index | `idx_events_ledger_desc` | CREATE | **Active** |
| `20260325000001_composite_indices` | Index | `idx_events_contract_ledger` | CREATE | **Potentially redundant** — see §2 |
| `20260325000001_composite_indices` | Index | `idx_events_tx_ledger` | CREATE | **Active** |
| `20260424000000_gin_index_event_data` | GIN index | `idx_events_event_data_gin` | CREATE CONCURRENTLY | **Active** |
| `20260425000001_event_data_validation` | CHECK constraint | `check_event_data_structure` | ALTER TABLE | **Active** |
| `20260426000000_create_indexer_checkpoints` | Table | `indexer_checkpoints` | CREATE | **Active** |
| `20260427000000_add_schema_version` | Table | `schema_version` | CREATE | **Active** |
| `20260427000001_event_data_compression` | Column | `events.event_data_compressed` | ADD COLUMN | **Active** |
| `20260427000002_normalization` | Table | `normalized_events` | CREATE | **Active** |
| `20260427000003_subscriptions` | Table | `subscriptions` | CREATE | **Active** |
| `20260427000004_contract_schemas` | Table | `contract_schemas` | CREATE | **Active** |
| `20260427000005_create_indexer_state` | Table | `indexer_state` | CREATE | **Active** |
| `20260427000006_event_data_size_limit` | CHECK constraint | size limit | ALTER TABLE | **Active** |
| `20260427000007_in_successful_call` | Column | `events.in_successful_contract_call` | ADD COLUMN | **Active** |
| `20260427000008_ledger_hash` | Column | `events.ledger_hash` | ADD COLUMN | **Active** |
| `20260427000009_contract_abis` | Table | `contract_abis` | CREATE | **Active** |
| `20260427000010_cursor_pagination_index` | Index | `idx_events_created_at_id` | CREATE | **Active** |
| `20260428000000_topic_0_sym` | Generated column + index | `topic_0_sym` | ADD | **Active** |
| `20260428000001_anonymized` | Table | `anonymized_events` | CREATE | **Active** |
| `20260428000002_matview_daily_summary` | Materialized view | `events_daily_summary` | CREATE | **Active** |
| `20260428000003_matview_contract_summary` | Materialized view | `events_contract_summary` | CREATE | **Potentially redundant** — `mv_contract_summary` (20260530) is a superset |
| `20260428000004_matview_hourly_volume` | Materialized view | `events_hourly_volume` | CREATE | **Active** |
| `20260429000000_event_data_fulltext_search` | Index | `idx_events_fulltext` | CREATE | **Active** |
| `20260430000000_add_tenant_id` | Column | `events.tenant_id` | ADD COLUMN | **Active** |
| `20260527000000_add_timestamp_index` | Index | `idx_events_timestamp` | CREATE | **Active** |
| `20260527000000_indexer_bloom_state` | Table | `indexer_bloom_state` | CREATE | **Active** |
| `20260527000001_rls_events` | RLS policies | `events` | ENABLE | **Active** |
| `20260527000002_webhook_failures` | Table | `webhook_failures` | CREATE | **Active** |
| `20260527000003_gin_index_event_data_topic` | GIN index | `idx_events_event_data_topic_gin` | CREATE | **Potentially redundant** — see §3 |
| `20260527024126_add_composite_indexes` | Index | `idx_events_contract_type_ledger` | CREATE | **Active** |
| `20260527024126_add_composite_indexes` | Index | `idx_events_type_ledger` | CREATE | **Active** |
| `20260527024126_add_composite_indexes` | Index | `idx_events_contract_type_partial` | CREATE | **Active** |
| `20260530000000_topic_1_2_3_gin` | GIN indexes | `idx_events_topic_1/2/3_gin` | CREATE | **Active** |
| `20260530000001_mv_contract_summary` | Materialized view | `mv_contract_summary` | CREATE | **Active** |
| `20260530000001_add_notification_channels` | Table | `notification_channels` | CREATE | **Active** |
| `20260530000002_add_saved_queries` | Table | `saved_queries` | CREATE | **Active** |
| `20260627000001_add_event_fingerprint` | Column + index | `events.content_fingerprint` | ADD | **Active** |
| `20260627000001_feature_flags` | Table | `feature_flags` | CREATE | **Active** |
| `20260628000001_abi_caching` | Table | `abi_cache` | CREATE | **Active** |
| `20260628000002_ledger_hashes_table` | Table | `ledger_hashes` | CREATE | **Active** |
| `20260628000003_multi_chain` | Table | `chain_configs` | CREATE | **Active** |
| `20260628000004_event_data_gzip` | Column | `events.event_data_gzip` | ADD COLUMN | **Active** |
| `20260629000001_sse_reconnect_and_query_cache` | Table | `query_cache` | CREATE | **Active** |
| `20260629000001_webhook_retry_queue` | Table | `webhook_retry_queue` | CREATE | **Active** |
| `20260630000002_schema_versioning_and_metrics` | Table | `schema_metrics` | CREATE | **Active** |
| `20260701000001_github_integration` | Table | `github_integrations` | CREATE | **Active** |
| `20260701000002_discord_integration` | Table | `discord_integrations` | CREATE | **Active** |
| `20260701000003_slack_integration` | Table | `slack_integrations` | CREATE | **Active** |
| `20260701000004_telegram_integration` | Table | `telegram_integrations` | CREATE | **Active** |
| `20260727000001_add_computed_columns_contracts` | Columns | contracts computed | ADD | **Active** |
| `20260727000001_webhook_templates` | Table | `webhook_templates` | CREATE | **Active** |
| `20260727000002_event_replay` | Table | `event_replay_jobs` | CREATE | **Active** |
| `20260727000002_partition_events_by_month` | Partitioned table | `events` (15 child tables) | RENAME+PARTITION | **Active** |
| `20260727000003_event_aggregation` | Table | `event_aggregations` | CREATE | **Active** |
| `20260727000003_statistics_auto_analysis` | Functions | `auto_analyze_*` | CREATE | **Active** |
| `20260727000004_anomaly_detection` | Table | `anomaly_detection_rules` | CREATE | **Active** |
| `20260727000005_webhook_endpoint_rate_limits` | Table | `webhook_endpoint_rate_limits` | CREATE | **Active** |

---

## 2. Index Redundancy Report

### Pair A — `idx_events_contract_ledger` vs `idx_events_contract_type_ledger`

| Index | Columns |
|-------|---------|
| `idx_events_contract_ledger` | `(contract_id, ledger DESC)` |
| `idx_events_contract_type_ledger` | `(contract_id, event_type, ledger DESC)` |

`idx_events_contract_type_ledger` is a strict superset. PostgreSQL can use it for
`(contract_id, ledger DESC)` queries with no `event_type` predicate by scanning
the leading two columns. The shorter index adds one extra write per insert with
no unique benefit.

**Recommendation: Drop `idx_events_contract_ledger`.**
Captured in `20260728000001_index_consolidation.sql`.

---

### Pair B — `idx_events_ledger_desc` vs `idx_events_contract_ledger`

| Index | Columns |
|-------|---------|
| `idx_events_ledger_desc` | `(ledger DESC)` |
| `idx_events_contract_ledger` | `(contract_id, ledger DESC)` |

`idx_events_ledger_desc` is the only index that serves the global paginated list
(`GET /v1/events`, no contract filter). The composite cannot substitute here.

**Recommendation: Keep Both** (but `idx_events_contract_ledger` is being dropped
for Pair A reasons — `idx_events_ledger_desc` remains).

---

### Pair C — `idx_events_tx_ledger` vs `idx_events_tx_hash_contract` (unique)

| Index | Columns |
|-------|---------|
| `idx_events_tx_ledger` | `(tx_hash, ledger DESC)` |
| `idx_events_tx_hash_contract` | `(tx_hash, contract_id, event_type)` UNIQUE |

Different purposes: `idx_events_tx_ledger` supports ordered range scans;
`idx_events_tx_hash_contract` enforces deduplication and is the conflict target
for `ON CONFLICT DO NOTHING`.

**Recommendation: Keep Both.**

---

### Pair D — `idx_events_type_ledger` vs `idx_events_contract_type_partial`

| Index | Columns |
|-------|---------|
| `idx_events_type_ledger` | `(event_type, ledger DESC)` |
| `idx_events_contract_type_partial` | `(ledger DESC) WHERE event_type = 'contract'` |

The partial index is narrower but faster for the exact `event_type = 'contract'`
pattern. Both were created in the same migration. Keep until a full
`INDEX_CHECK_INTERVAL_HOURS` cycle confirms `idx_events_contract_type_partial`
`idx_scan = 0`, then drop.

**Recommendation: Keep Both** for now; review after one monitoring cycle.

---

### Materialized view redundancy

`events_contract_summary` (20260428) and `mv_contract_summary` (20260530) both
aggregate by `contract_id`. The newer view contains richer columns. Verify no
handler queries `events_contract_summary` directly, then drop in a future migration.

---

## 3. GIN Index Analysis

| Index | Expression | Operator class |
|-------|-----------|----------------|
| `idx_events_event_data_gin` | `event_data` | `jsonb_path_ops` |
| `idx_events_event_data_topic_gin` | `event_data -> 'topic'` | default (`jsonb_ops`) |
| `idx_events_topic_1_gin` | `event_data->'topic'->1` | `jsonb_path_ops` |
| `idx_events_topic_2_gin` | `event_data->'topic'->2` | `jsonb_path_ops` |
| `idx_events_topic_3_gin` | `event_data->'topic'->3` | `jsonb_path_ops` |

`idx_events_event_data_gin` covers full-document `@>` containment queries including
topic containment. `idx_events_topic_1/2/3_gin` cover per-position topic queries
with the more compact `jsonb_path_ops` class.

`idx_events_event_data_topic_gin` (plain `jsonb_ops` on `event_data->'topic'`) is
covered by both groups above.

**Recommendation: Drop `idx_events_event_data_topic_gin`.**
Captured in `20260728000001_index_consolidation.sql`.

---

## 4. Partition Strategy Assessment

### 4.1 Child partitions created by `20260727000002_partition_events_by_month.sql`

15 monthly partitions from `events_2025_07` to `events_2026_09` (current date:
2026-07-28). Two future partitions (`2026_08`, `2026_09`) are pre-created.

### 4.2 `events_legacy` table

`events_legacy` is the original non-partitioned `events` table preserved during
the rename in `20260727000002_partition_events_by_month.sql`. A full grep of
`src/**/*.rs` confirms **zero references** to `events_legacy`. Safe to drop after
confirming data completeness.

### 4.3 Partition pruning effectiveness

The primary access patterns (`GET /v1/events/{contract_id}`) filter only on
`contract_id` — no `timestamp` predicate. Since partitioning is by `timestamp`,
the planner **cannot prune partitions** for these queries. All 15 child partitions
are scanned, but the per-partition `idx_events_contract_type_ledger` index limits
the actual row retrieval.

Queries with `from_ledger`/`to_ledger` can be converted to timestamp bounds by
the application layer to enable pruning — a future optimisation opportunity.

**Recommendation: Keep monthly partitioning.** It is the correct granularity for
time-range queries and retention management (drop old partitions cleanly).
