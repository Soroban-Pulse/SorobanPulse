# Runbook: Schema Consolidation

**Related issue:** #804  
**Related files:** `src/index_monitor.rs`, `docs/schema-audit.md`, `docs/schema.md`, `scripts/manage_partitions.sql`

---

## 1. Schema Health Check

The schema health check runs inside the `index_monitor` background task every
`INDEX_CHECK_INTERVAL_HOURS` (default: 24 h).  It emits two Prometheus gauges:

| Metric | Type | Meaning |
|--------|------|---------|
| `soroban_pulse_schema_unused_indexes_total` | gauge | Count of public-schema indexes with `idx_scan = 0` (excluding partition child indexes) |
| `soroban_pulse_schema_missing_future_partitions` | gauge | Count of next-2-month partitions not yet pre-created |

### Recommended alerts

```yaml
# Alert when unused indexes persist for more than 24 h
- alert: UnusedSchemaIndexes
  expr: soroban_pulse_schema_unused_indexes_total > 0
  for: 24h
  annotations:
    summary: "Unused indexes detected — review and drop to reduce write overhead"

# Alert when future partition pre-creation is lagging
- alert: MissingFuturePartitions
  expr: soroban_pulse_schema_missing_future_partitions > 0
  for: 48h
  annotations:
    summary: "Future month partitions not pre-created — run create_future_partitions(3)"
```

### Reading the gauges

```promql
# Current unused index count
soroban_pulse_schema_unused_indexes_total

# Missing future partitions
soroban_pulse_schema_missing_future_partitions
```

The check also emits `WARN` log lines naming each unused index and each missing
partition.  Search for `"schema health"` in logs:

```bash
grep "schema health" /var/log/soroban-pulse.log | tail -50
```

---

## 2. Adding a New Partition Manually

Run when the automated check fires `MissingFuturePartitions`:

```sql
-- Connect to the database
\c soroban_pulse

-- Create the next 3 months of partitions
SELECT create_future_partitions(3);

-- Verify they were created
SELECT tablename FROM pg_tables
WHERE schemaname = 'public' AND tablename LIKE 'events_20%'
ORDER BY tablename;
```

To create a single specific month:

```sql
-- Example: create the August 2026 partition
SELECT create_event_partition(2026, 8);
```

Both functions are defined in `scripts/manage_partitions.sql` and loaded into
the database on startup.

---

## 3. Dropping a Redundant Index Safely in Production

**Use `CONCURRENTLY` — never drop an index without it on a live table.**

```sql
-- Step 1: Verify the index is actually unused
SELECT indexname, idx_scan
FROM pg_stat_user_indexes
WHERE schemaname = 'public' AND indexname = 'idx_to_drop';
-- Confirm idx_scan = 0 (and has been 0 for at least one monitoring cycle)

-- Step 2: Confirm no query plan uses it
EXPLAIN (FORMAT TEXT)
SELECT id FROM events WHERE contract_id = $1 ORDER BY ledger DESC LIMIT 20;
-- Verify the plan does not reference idx_to_drop

-- Step 3: Drop concurrently (does not lock the table)
DROP INDEX CONCURRENTLY IF EXISTS idx_to_drop;

-- Step 4: Verify the drop completed
SELECT indexname FROM pg_indexes
WHERE schemaname = 'public' AND indexname = 'idx_to_drop';
-- Should return zero rows
```

### Rollback procedure

If query performance degrades after the drop:

```sql
-- Recreate the index concurrently (no table lock)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_to_drop
    ON events(contract_id, ledger DESC);
```

Monitor `soroban_pulse_query_duration_seconds` and
`soroban_pulse_http_request_duration_seconds` for 15 minutes after any index
change.

---

## 4. Materialized View Refresh Coordination

Three materialized views are refreshed by `src/stats_refresh.rs` on a
configurable interval (`STATS_REFRESH_INTERVAL_SECS`, default 300 s):

| View | Purpose |
|------|---------|
| `events_daily_summary` | Per-date event counts by type |
| `events_contract_summary` (legacy) | Per-contract totals (consider dropping — see `docs/schema-audit.md` §2) |
| `mv_contract_summary` | Rich per-contract aggregation |
| `events_hourly_volume` | Hourly event counts for last 7 days |

### Normal operation

Each refresh runs `REFRESH MATERIALIZED VIEW CONCURRENTLY` with a 5-second
lock timeout.  If a long-running query holds a conflicting lock, the refresh
is skipped and a `WARN` is logged.  The view is retried at the next interval —
stale data is served in the meantime.

### View is stale for > 30 minutes

1. Check for blocking queries:
   ```sql
   SELECT pid, query, state, wait_event_type, wait_event
   FROM pg_stat_activity
   WHERE state != 'idle'
   ORDER BY query_start;
   ```

2. If a query has been running > 10 minutes and is blocking the refresh,
   consider terminating it:
   ```sql
   SELECT pg_terminate_backend(<pid>);
   ```

3. Manually trigger a refresh:
   ```sql
   REFRESH MATERIALIZED VIEW CONCURRENTLY mv_contract_summary;
   REFRESH MATERIALIZED VIEW CONCURRENTLY events_daily_summary;
   REFRESH MATERIALIZED VIEW CONCURRENTLY events_hourly_volume;
   ```

4. Confirm the `soroban_pulse_matview_refresh_duration_seconds` histogram
   shows a successful refresh.

---

## 5. Interpreting `docs/schema-audit.md`

The schema audit document (`docs/schema-audit.md`) has four sections:

| Section | What it tells you |
|---------|-------------------|
| §1 Migration Inventory | Every migration file, what it creates/alters, and whether the object is still active or superseded |
| §2 Index Redundancy Report | Pairs of indexes with overlapping column prefixes; recommendation to keep/drop each |
| §3 GIN Index Analysis | Which GIN indexes overlap and which can be dropped |
| §4 Partition Strategy Assessment | Child partition list, `events_legacy` status, partition pruning effectiveness |

### How to update the audit after new migrations

1. Add a row to the §1 table for the new migration.
2. If the migration adds an index, check §2 for any new overlapping pair.
3. If the migration adds a GIN index, update §3.
4. If the migration adds partitions or changes the partition key, update §4.
5. Update the `Date:` at the top of `schema-audit.md`.

The audit is a living document — keep it current with each PR that changes
the schema.

---

## See Also

- `docs/schema.md` — canonical schema reference with consolidation history
- `docs/schema-audit.md` — full migration inventory and index redundancy report
- `scripts/manage_partitions.sql` — partition management SQL functions
- `src/index_monitor.rs` — schema health check implementation
- `docs/runbooks/db-pool-exhaustion.md` — pool exhaustion can delay health checks
