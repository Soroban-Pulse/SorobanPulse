# Migrating from Stellar Horizon (Issue #1000)

> Implementation-only, not test-verified — see the warning in
> [README.md](README.md).

This guide is for teams currently polling or streaming Horizon's
transaction/effects/operations endpoints to watch for Soroban contract
activity, who want to move that workflow onto Soroban Pulse's
purpose-built contract-event indexing and subscription/notification layer.

## Why teams migrate

Horizon's event surface is transaction/operation/effect-centric and general
to the whole ledger. Soroban Pulse instead indexes decoded Soroban contract
*events* directly and adds subscription filtering, delivery guarantees
(retry, webhook signing), and multi-channel notification (webhook, email,
SMS, push, chat integrations) on top — see [architecture.md](../architecture.md)
and [FAQ.md](../FAQ.md).

## Conceptual mapping

| Horizon concept | Soroban Pulse equivalent |
|---|---|
| Polling `/transactions` or `/effects` for a contract | A `subscriptions` row with a `contract_filter` (see [subscription-mapping.md](subscription-mapping.md)) |
| Streaming via Horizon SSE (`Accept: text/event-stream`) | Soroban Pulse's own SSE stream — see [event-feeds.md](../event-feeds.md) and [sse-reconnection.md](../sse-reconnection.md) |
| Cursor-based pagination (`cursor` param) | `from_ledger` / `acked_ledger` on the subscription, plus standard REST pagination — see [api-guide.md](../api-guide.md) |
| Custom webhook relay you built on top of Horizon polling | Native webhook delivery with HMAC signing and retries — see [webhook-signing.md](../webhook-signing.md), [retry-policies.md](../retry-policies.md) |
| Manually decoding XDR event data | Soroban Pulse decodes and validates XDR before storage (`record_xdr_validation_pass`/`_fail` in `metrics.rs`) |

## Migration steps

1. **Inventory your current Horizon consumers.** List every process
   polling/streaming Horizon for Soroban activity, the contracts each one
   watches, and where results currently get delivered (your own webhook
   relay, a queue, a database).
2. **Recreate each consumer as a Soroban Pulse subscription.** Follow
   [subscription-mapping.md](subscription-mapping.md) to translate each
   consumer's contract filter and delivery target.
3. **Backfill history**, if your consumers need historical events and not
   just events going forward. See
   [data-migration-procedures.md](data-migration-procedures.md).
4. **Run in parallel.** Point new subscriptions at a staging/canary delivery
   target first (e.g. a webhook endpoint that just logs), and compare
   against what your existing Horizon-based consumer sees for the same
   ledger range — see [validation.md](validation.md).
5. **Cut over.** Once validated, repoint delivery targets at production
   endpoints and stop polling Horizon for the migrated contracts. Keep the
   old consumer's code available (even if stopped) until you're confident —
   see [rollback-procedures.md](rollback-procedures.md).
6. **Decommission the old consumer.**

## Cutover strategy

Prefer a **shadow period** over a hard cutover:

- Run both the legacy Horizon-based consumer and the new Soroban Pulse
  subscription simultaneously for a fixed window (a few days of production
  traffic, long enough to see your real event-volume variance).
- Compare event counts and identities (contract ID + ledger + tx hash) —
  see the reconciliation query in [validation.md](validation.md).
- Only disable the legacy consumer once the shadow period shows no
  unexplained discrepancies.

## Common pitfalls

- **Ledger range gaps**: if your Horizon consumer's last processed ledger
  and Soroban Pulse's indexer start point don't overlap, you'll have a gap.
  Confirm Soroban Pulse's `soroban_pulse_indexer_current_ledger` was at or
  below your last Horizon-processed ledger before considering the backfill
  complete.
- **Different event identity**: Horizon operation/effect IDs are not the
  same as Soroban Pulse's internal event IDs. Reconcile by `(contract_id,
  ledger, tx_hash)`, not by ID.
- **Rate limits**: Horizon and the underlying Soroban RPC have independent
  rate limits; don't assume your existing backoff tuning transfers directly
  — see [performance-tuning.md](../performance-tuning.md).
