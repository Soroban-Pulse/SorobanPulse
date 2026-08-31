# Migration Guides (Issue #1000)

> **Status: implementation only, not test-verified.** These guides were
> written to satisfy the issue #1000 checklist without running an actual
> migration end-to-end. Treat every command, query, and mapping below as
> **unreviewed and unverified** — dry-run and review with a DBA before using
> against production data.
>
> **Warning:** several documents in this directory describe destructive or
> hard-to-reverse operations (bulk inserts, cutovers, schema changes). Do not
> run anything here against a production database without a tested backup
> and a rollback plan you have personally verified — see
> [rollback-procedures.md](rollback-procedures.md).

Guides for moving event-watching and notification workflows onto Soroban
Pulse from another system.

## Contents

1. [Migrating from Stellar Horizon](from-stellar-horizon.md) — moving
   contract-event polling/streaming off Horizon's transaction/effects API.
2. [Migrating from other event indexers](from-other-indexers.md) — general
   guidance for indexers not covered by a dedicated guide (custom RPC
   pollers, other third-party Soroban indexers).
3. [Subscription mapping](subscription-mapping.md) — how filters,
   callback/webhook config, and delivery channels map onto Soroban Pulse's
   subscription model.
4. [Data migration procedures](data-migration-procedures.md) — how to
   backfill historical events into Soroban Pulse's PostgreSQL schema.
5. [Migration validation](validation.md) — how to confirm a migration is
   complete and correct before decommissioning the old system.
6. [Rollback procedures](rollback-procedures.md) — how to revert a
   migration safely if validation fails or production issues appear.

## Suggested order of operations

```
1. Read subscription-mapping.md to plan how your existing subscriptions
   translate.
2. Run the backfill in data-migration-procedures.md against a staging
   database first.
3. Run every check in validation.md against staging.
4. Only then repeat against production, running old and new systems
   in parallel (see "Cutover strategy" in from-stellar-horizon.md or
   from-other-indexers.md).
5. Keep rollback-procedures.md open during cutover — know how you'd
   revert before you need to.
```
