# Migration validation (Issue #1000)

> Implementation-only, not test-verified — see the warning in
> [README.md](README.md).

How to confirm a migration (subscription mapping and/or historical
backfill) is complete and correct before decommissioning the old system.
Run every check in this document against **staging first**, then again
against production before cutover.

## Count reconciliation

Compare event counts between the old system and Soroban Pulse for the same
ledger range, grouped by contract and day:

```sql
SELECT contract_id, date_trunc('day', to_timestamp(ledger_closed_at_epoch)) AS day, count(*)
FROM events
WHERE ledger BETWEEN $start AND $end
GROUP BY 1, 2
ORDER BY 1, 2;
```

Run the equivalent query/export against the old system and diff the
results. Investigate every discrepancy — don't assume small differences are
noise; they're usually either a filter mismatch or a real gap.

## Identity reconciliation

Counts matching isn't sufficient — confirm the *same* events are present,
keyed by `(contract_id, ledger, tx_hash)`:

```sql
SELECT contract_id, ledger, tx_hash FROM events
WHERE ledger BETWEEN $start AND $end
EXCEPT
SELECT contract_id, ledger, tx_hash FROM events_backfill_staging
WHERE ledger BETWEEN $start AND $end;
```

Run in both directions (staging minus production, and production minus
staging) to catch both missing and unexpected extra rows.

## Spot-check event payloads

Counts and identities matching doesn't confirm the *content* is right.
Pick a random sample (aim for statistical coverage of your event-type
distribution, not just the first N rows) and diff the decoded `event_data`
field-by-field against the source system's representation of the same
event.

## Subscription behavior validation

For each migrated subscription (see
[subscription-mapping.md](subscription-mapping.md)):

- [ ] Trigger (or wait for) a real matching event and confirm delivery
      reaches the same downstream consumer the old system fed.
- [ ] Confirm filter behavior matches — an event that the old consumer
      would have *ignored* should also be filtered out by the new
      subscription (false positives are as much a bug as false negatives).
- [ ] Confirm delivery channel configuration (webhook signature, email
      schedule, SMS number, push token) is correct — see
      [webhook-verification.md](../webhook-verification.md) for verifying
      webhook deliveries specifically.
- [ ] Confirm priority/urgency handling matches the old system's for any
      event types that were treated as urgent — see
      [priority-queueing.md](../priority-queueing.md).

## Operational validation

- [ ] `soroban_pulse_indexer_lag_ledgers` is within the same tolerance the
      old system offered (or better) — see
      [metrics-reference.md](../metrics-reference.md).
- [ ] No unexpected spike in `soroban_pulse_events_validation_failed_total`
      or `soroban_pulse_events_xdr_invalid_total` for the migrated
      contracts.
- [ ] No unexpected spike in `soroban_pulse_notification_delivery_failure_total`
      for the migrated subscriptions' channels.
- [ ] Load-test the migrated subscription set at expected production volume
      before full cutover — see
      [load-testing-runbook.md](../load-testing-runbook.md).

## Sign-off checklist

Before decommissioning the old system:

- [ ] Count reconciliation shows zero unexplained discrepancies for a full
      production traffic window (not just staging).
- [ ] Identity reconciliation shows zero unexplained rows in either
      direction.
- [ ] Every migrated subscription has been confirmed to deliver correctly
      at least once against a real production event.
- [ ] The team has reviewed [rollback-procedures.md](rollback-procedures.md)
      and knows how to revert if something surfaces post-cutover.
- [ ] A decommission date for the old system is agreed and communicated —
      don't decommission the moment validation passes; keep it available
      (even if idle) through at least one full on-call rotation.
