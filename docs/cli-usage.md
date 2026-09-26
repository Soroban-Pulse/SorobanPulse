# `spulse` CLI Usage

_Issue #964_

`spulse` is the command-line client for the Soroban Pulse API — see
[`cli/README.md`](../cli/README.md) for install/build instructions. This
page covers the commands added to round it out into a full API client:
subscription management, a webhook test command, and configuration.

## Configuration

```bash
spulse config set base_url https://pulse.example.com
spulse config set api_key sk_live_...
spulse config show
spulse config path      # print the config file location
```

Every command also accepts `--base-url` / `--api-key` (or the
`SPULSE_BASE_URL` / `SPULSE_API_KEY` env vars) to override the saved config
for a single invocation.

## Subscriptions

Create, inspect, and manage event subscriptions without hand-writing curl:

```bash
# Create a subscription starting at ledger 1,000,000
spulse subscriptions create \
  --callback-url https://example.com/webhooks/pulse \
  --from-ledger 1000000

# Batch delivery instead of one event per request
spulse subscriptions create \
  --callback-url https://example.com/webhooks/pulse \
  --from-ledger 1000000 \
  --subscription-type batch \
  --batch-size 50 \
  --batch-timeout-ms 5000

# Inspect / cancel
spulse subscriptions get <id>
spulse subscriptions delete <id>

# Advance the acked cursor after processing events up to a ledger
spulse subscriptions ack <id> --ledger 1000042

# Pause and resume delivery without losing the subscription
spulse subscriptions pause <id> --seconds 3600 --reason "maintenance"
spulse subscriptions resume <id>
```

## Testing a webhook receiver

`webhook-test` sends a single synthetic event — shaped like a real
delivery payload, flagged with `test_delivery: true` and an
`x-soroban-pulse-test: true` header — directly to a URL, so you can verify
your receiver is reachable and returns 2xx *before* pointing a live
subscription at it:

```bash
spulse webhook-test https://example.com/webhooks/pulse
spulse webhook-test https://example.com/webhooks/pulse --contract CABC... --timeout 5
```

It prints the HTTP status, round-trip latency, and a snippet of the
response body, and exits non-zero if the receiver didn't return 2xx —
useful as a pre-flight check in a deploy script.

Note this talks straight to the given URL; it does not go through the
Soroban Pulse API or use your configured `base_url`/`api_key`.

## Events, contracts, stats, export

These predate this page but are documented here for completeness — see
`spulse <command> --help` for the full flag list:

```bash
spulse events --contract CABC... --limit 50 --sort-by ledger --sort desc
spulse contracts --search CABC
spulse stats --contract CABC...
spulse export --output events.jsonl --format jsonl --from-ledger 1000000
```
