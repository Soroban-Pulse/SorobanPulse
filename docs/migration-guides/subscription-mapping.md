# Subscription mapping (Issue #1000)

> Implementation-only, not test-verified — see the warning in
> [README.md](README.md). Column names below reflect the base schema in
> `migrations/20260427000003_subscriptions.sql` plus fields referenced
> elsewhere in this codebase (`src/push_notification.rs`, `src/email.rs`,
> `src/sms.rs`); later migrations may have added columns not enumerated
> here — check `migrations/` for the current full schema before relying on
> this table.

How to translate an existing consumer (from Horizon, another indexer, or a
hand-rolled poller) into a Soroban Pulse subscription.

## Core subscription fields

| Field | Purpose | Migration notes |
|---|---|---|
| `callback_url` | Webhook delivery target | Direct mapping if your existing consumer already receives webhooks. |
| `from_ledger` | Ledger to start delivering from | Set to the ledger *after* the last one your old system fully processed, to avoid duplicate delivery during a shadow-run — see [data-migration-procedures.md](data-migration-procedures.md). |
| `acked_ledger` | Last ledger the client has acknowledged | Leave at the default (`0`) for a new subscription; this is maintained by Soroban Pulse, not set by you. |
| `status` | `active` \| `cancelled` | Create new subscriptions as `active`; don't reuse a `cancelled` row. |
| `contract_filter` | Which contract(s) to watch | Maps from whatever contract-scoping your old consumer used (a hardcoded contract ID list, a config file, etc.). See [filter-dsl.md](../filter-dsl.md) for filter expressiveness beyond a flat contract list. |

## Delivery-channel-specific fields

These map to the notifier configs in `src/email.rs`, `src/sms.rs`, and
`src/push_notification.rs` — see
[notification-architecture.md](../notification-architecture.md) for how
delivery is implemented once a subscription is configured for a channel.

| Old system had... | Maps to... |
|---|---|
| An internal webhook relay | `callback_url` + HMAC verification per [webhook-verification.md](../webhook-verification.md) |
| An email digest job | Email notification config — `schedule` (`immediate` / `hourly_digest` / `daily_digest` / custom cron), `quiet_hours` — see [email-notifications.md](../email-notifications.md) |
| An SMS alerting integration (e.g. Twilio direct) | SMS notification config — reuse the same Twilio account/number if you already have one provisioned, or provision a new one per [email-quick-start.md](../email-quick-start.md)-style onboarding (SMS equivalent) |
| A mobile push integration (FCM/APNs) | Push token registration via `PUT /v1/subscriptions/{id}/push` — see `update_subscription_push` in `src/push_notification.rs` |
| A Slack/Discord/Teams bot posting alerts | The corresponding chat integration doc — [slack-integration.md](../slack-integration.md), [discord-integration.md](../discord-integration.md), [teams-integration.md](../teams-integration.md) |

## Filter translation checklist

- [ ] List every contract ID the old consumer watched.
- [ ] List every event-type / field-level filter the old consumer applied
      (in code, config, or a downstream filter step) — translate what's
      expressible via [filter-dsl.md](../filter-dsl.md) into the
      subscription's filter; anything not expressible stays a downstream
      concern on the consumer side.
- [ ] Confirm priority handling — if the old consumer treated some event
      types as urgent, configure Soroban Pulse's priority rules so those
      still bypass batching (see [priority-queueing.md](../priority-queueing.md)
      and the `critical` priority path in `src/email.rs`).
- [ ] Confirm rate-limit expectations carry over — see
      [notification-rate-limiting.md](../notification-rate-limiting.md).

## Worked example

Old system: a cron job polling Horizon every 10s for a single contract's
mint events, POSTing matches to an internal Slack webhook.

Soroban Pulse subscription:

```
contract_filter: ["CABCDEF...CONTRACT_ID"]
callback_url:    (leave unset — using a chat integration instead)
```

Plus a Slack integration configured per
[slack-integration.md](../slack-integration.md) scoped to the same contract
filter, with an event-type filter for `mint` per
[filter-dsl.md](../filter-dsl.md).
