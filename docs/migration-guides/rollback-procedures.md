# Rollback procedures (Issue #1000)

> Implementation-only, not test-verified — see the warning in
> [README.md](README.md). **Every command below is illustrative and has not
> been executed against a real database as part of this change.** Test each
> procedure against a disposable copy of your data before you need it for
> real — a rollback procedure you've never run is not a rollback procedure
> you can trust under pressure.

How to revert a Soroban Pulse migration if validation fails, or if a
problem surfaces after cutover. Keep this document open (not just linked)
during any cutover window.

## Principle: don't decommission the old system early

The single most effective rollback strategy is not needing one of these
procedures at all — see the sign-off checklist in
[validation.md](validation.md). Keep the old system available (even fully
stopped/idle) until you're well past the point where a rollback would
plausibly be needed.

## Rollback scenario: bad backfill data

If [validation.md](validation.md) or post-promotion monitoring surfaces
incorrect historical data:

1. **Stop.** Don't let more traffic build on top of bad data — pause any
   process still writing to the affected table/range.
2. **Identify the exact affected range** (contract(s), ledger range,
   insertion batch/timestamp) from your backfill's own staging-table
   records (see [data-migration-procedures.md](data-migration-procedures.md)
   — this is why staging tables are recommended over direct inserts).
3. **Delete the affected rows**, scoped as narrowly as possible:

   ```sql
   DELETE FROM events
   WHERE contract_id = $contract_id
     AND ledger BETWEEN $start AND $end
     AND created_at >= $backfill_started_at;
   ```

   Prefer scoping by `created_at >= $backfill_started_at` in addition to
   the ledger range, so you don't accidentally delete legitimately
   live-indexed rows that happen to share a ledger range with the bad
   backfill.

4. **Re-run the backfill** for the affected range once the root cause is
   fixed, following [data-migration-procedures.md](data-migration-procedures.md)
   again from the staging step.

## Rollback scenario: a migrated subscription is misbehaving

If a specific subscription created during migration is misdelivering
(wrong filter, wrong channel, duplicate delivery vs. the old system still
running):

1. **Set the subscription's `status` to `cancelled`** rather than deleting
   it — this stops delivery immediately while preserving the row for
   post-mortem:

   ```sql
   UPDATE subscriptions SET status = 'cancelled' WHERE id = $subscription_id;
   ```

2. **Re-enable the old system's equivalent consumer** for that specific
   contract/use case if it was already disabled.
3. **Fix the mapping** per [subscription-mapping.md](subscription-mapping.md)
   and re-validate in staging before re-enabling.

## Rollback scenario: full cutover needs to be reverted

If a broad set of consumers were cut over and something is wrong broadly
enough that reverting individual subscriptions isn't practical:

1. **Re-point delivery** back to the old system for affected consumers —
   this is why the shadow-run strategy in
   [from-stellar-horizon.md](from-stellar-horizon.md) /
   [from-other-indexers.md](from-other-indexers.md) recommends keeping the
   old system's delivery paths intact (not deleted) through cutover.
2. **Cancel** (don't delete) the migrated subscriptions per the single
   subscription procedure above, so they stop delivering while you
   investigate.
3. **Leave backfilled historical data in place** unless it's specifically
   implicated (see the backfill rollback scenario above) — reverting
   delivery doesn't require reverting history.
4. **Do a blameless post-mortem** before attempting cutover again; update
   [validation.md](validation.md) with whatever check would have caught the
   issue.

## What NOT to do

- Don't `DELETE FROM events` unscoped, or with only a time bound and no
  contract/range bound — always scope by the specific migration batch.
- Don't drop the old system's infrastructure until well past sign-off (see
  [validation.md](validation.md)'s sign-off checklist) — a truncated or
  destroyed old system removes your rollback option entirely.
- Don't treat a partial rollback as a substitute for root-causing the
  issue — re-attempting the same cutover without a fix just reproduces the
  problem.
