# Data migration procedures (Issue #1000)

> Implementation-only, not test-verified — see the warning in
> [README.md](README.md). The SQL and CLI invocations below are illustrative
> and have **not** been executed against a real database as part of this
> change. Dry-run against a disposable copy of your data before using them
> for real.

Procedures for backfilling historical event data into Soroban Pulse's
PostgreSQL schema (`events` table, see
`migrations/20260314000000_create_events.sql` and later migrations) when
migrating from another system.

## Decide whether you need a backfill at all

Skip this entire document if downstream consumers only need events *going
forward* — just create subscriptions with `from_ledger` set to the current
ledger and start there. A backfill is only needed when consumers require
historical event data inside Soroban Pulse itself (e.g. for its query API
or dashboards), not just forward delivery.

## Backfill sources, in order of preference

1. **Your old indexer's own storage**, if it kept structured events — fastest
   and gives you a natural comparison baseline for
   [validation.md](validation.md).
2. **Soroban RPC's `getEvents`**, replayed for the ledger range you need —
   use this when your old system didn't retain structured history, or you
   don't trust its correctness enough to seed from it. See
   [contract-event-simulation.md](../contract-event-simulation.md) for
   related tooling patterns.
3. **Stellar Horizon's historical archives**, if migrating off Horizon and
   RPC retention doesn't reach far enough back.

## Procedure: backfill from a structured source

1. **Stage the data**, don't write directly into `events`. Load into a
   staging table with the same shape (or close enough to be adapted with a
   view), so a bad batch can be discarded without touching production data:

   ```sql
   CREATE TABLE events_backfill_staging (LIKE events INCLUDING ALL);
   ```

2. **Transform** old-system records into Soroban Pulse's `events` shape.
   At minimum this means producing a valid `contract_id`, `ledger`,
   `tx_hash`, `event_type`, and the XDR-derived `event_data` payload Soroban
   Pulse expects — validate each record with the same XDR validation path
   the live indexer uses (`record_xdr_validation_pass`/`_fail` in
   `src/metrics.rs`) rather than trusting the old system's shape blindly.

3. **Load in ledger-ordered batches**, not one giant transaction — this
   keeps a failed batch small to retry and avoids long-held locks on a live
   table:

   ```sql
   INSERT INTO events_backfill_staging (...)
   SELECT ... FROM old_system_export
   WHERE ledger BETWEEN $start AND $end
   ORDER BY ledger;
   ```

4. **Deduplicate against events already indexed live** (relevant if the
   live indexer and the backfill's ledger range overlap during a shadow
   run):

   ```sql
   DELETE FROM events_backfill_staging s
   USING events e
   WHERE s.contract_id = e.contract_id
     AND s.ledger = e.ledger
     AND s.tx_hash = e.tx_hash;
   ```

5. **Validate the staging table** — see
   [validation.md](validation.md) before promoting.

6. **Promote** in one final batch insert, during a low-traffic window:

   ```sql
   INSERT INTO events SELECT * FROM events_backfill_staging
   ON CONFLICT DO NOTHING;
   ```

7. **Drop the staging table** once promotion is confirmed correct.

## Procedure: backfill via Soroban RPC replay

Prefer running this through the indexer's own ingestion path (temporarily
pointed at a historical `from_ledger`) rather than a bespoke script, so
every event goes through the same XDR validation, dedup, and normalization
logic (`src/normalizer.rs`, `src/dedup.rs`) the live path uses. Running a
separate ad-hoc script risks producing rows that don't match what the live
indexer would have produced for the same ledger.

1. Confirm the target RPC endpoint's retention actually covers the ledger
   range you need — RPC nodes often have much shorter retention than
   Horizon archives.
2. Run the indexer against a **separate database** (or a clearly-scoped
   staging schema) with `from_ledger`/`to_ledger` bounding the backfill
   range, so it doesn't interfere with live indexing.
3. Follow the "Load in ledger-ordered batches" → "Deduplicate" → "Validate"
   → "Promote" steps above, using this staging database as the source.

## Table partitioning consideration

If you're backfilling a large historical range, check
[table-partitioning.md](../table-partitioning.md) and
[data-retention-tiers.md](../data-retention-tiers.md) first — inserting
years of history into an unpartitioned table you plan to partition later is
more work than partitioning before the backfill.

## After the backfill

Run every check in [validation.md](validation.md), then update
`soroban_pulse_migrations_applied_total` / operational runbooks to reflect
that history now lives in Soroban Pulse, per your team's normal change-log
process.
