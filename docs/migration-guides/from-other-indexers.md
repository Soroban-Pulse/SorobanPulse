# Migrating from other event indexers (Issue #1000)

> Implementation-only, not test-verified — see the warning in
> [README.md](README.md).

This guide is deliberately generic — for a source system without a
dedicated guide (a custom-built Soroban RPC poller, an internal indexer, or
a third-party indexer). If you're specifically migrating off Stellar
Horizon, use [from-stellar-horizon.md](from-stellar-horizon.md) instead,
which is more specific.

## Before you start

Answer these questions about your current system — every later step depends
on them:

1. **What triggers an event to be captured?** (Polling interval? RPC
   subscription? Another indexer's webhook?)
2. **What's the unit of identity for an event?** You'll need this to
   reconcile old vs. new during validation — Soroban Pulse identifies events
   by `(contract_id, ledger, tx_hash)` plus an internal UUID.
3. **How far back does your current system's history go, and do downstream
   consumers need that full history in Soroban Pulse, or only events going
   forward?**
4. **How is delivery currently done** (webhook, queue, database polling by
   consumers, email/SMS/push)? See
   [subscription-mapping.md](subscription-mapping.md) for how each maps.

## Migration steps

1. **Stand up Soroban Pulse alongside your existing indexer** — see
   [development-setup.md](../development-setup.md) /
   [deployment.md](../deployment.md). Do not touch the existing system yet.
2. **Recreate subscriptions.** For each downstream consumer of your current
   indexer, create an equivalent Soroban Pulse subscription — see
   [subscription-mapping.md](subscription-mapping.md).
3. **Backfill if needed** — see
   [data-migration-procedures.md](data-migration-procedures.md). If your
   current indexer's events are themselves queryable (a database or API),
   prefer sourcing the backfill from there over re-deriving from raw ledger
   data, since it's usually faster and gives you a natural point of
   comparison for validation.
4. **Validate** — see [validation.md](validation.md). At minimum, confirm
   event counts per contract per day match between old and new systems for
   an overlapping window.
5. **Shadow-run**, same as the Horizon guide's cutover strategy: run both
   systems, compare live event delivery, then cut over per-consumer rather
   than all at once so a bad translation for one consumer doesn't block the
   rest.
6. **Decommission** the old indexer once every consumer has been cut over
   and validated.

## Mapping worksheet

Use this table while planning; fill in one row per existing consumer.

| Existing consumer | Contracts watched | Filter logic | Current delivery target | Soroban Pulse subscription filter | Soroban Pulse delivery channel |
|---|---|---|---|---|---|
| _(example)_ ops-alerts-bot | CABCDEF... | event_type = mint | internal webhook | `contract_filter: ["CABCDEF..."]`, event type filter per [filter-dsl.md](../filter-dsl.md) | webhook, see [webhook-signing.md](../webhook-signing.md) |

## Common pitfalls

- **Filter semantics don't translate 1:1.** A custom indexer's ad-hoc filter
  logic (e.g. regex on event payload) may not have a direct equivalent —
  check [filter-dsl.md](../filter-dsl.md) for what's expressible, and lean
  on subscription-side filtering rather than trying to replicate complex
  post-processing that belongs downstream.
- **At-least-once vs. at-most-once assumptions.** Confirm whether your
  current consumers assume exactly-once delivery; Soroban Pulse's retry
  behavior is at-least-once (idempotency keys are available — see
  [idempotency.md](../idempotency.md)).
- **Timezone/format mismatches** in timestamps when reconciling event
  counts — Soroban Pulse stores/reports in UTC throughout.
