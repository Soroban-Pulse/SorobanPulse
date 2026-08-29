# Local API Sandbox

Two docker-compose stacks exist in this repo. Neither is a purpose-built "API
sandbox" product — this doc explains what's actually there and how to point
either one at test data.

---

## Option 1: `docker-compose.yml` — full stack against Stellar testnet

```bash
docker compose up --build --wait
```

Brings up Postgres, the app (indexing `https://soroban-testnet.stellar.org`
by default), Prometheus, Grafana, and Redis. `START_LEDGER=0` means the
indexer starts from the current tip of testnet, so:

- Data is **real but unpredictable** — you're at the mercy of whatever
  contracts are emitting events on testnet at the moment.
- There is no seed step; the indexer populates `events` as it ingests new
  ledgers, which can take a few minutes to accumulate anything for a specific
  contract.

Useful for smoke-testing against real Soroban RPC behavior, not for
deterministic examples or CI.

```bash
curl http://localhost:3000/healthz/ready
curl http://localhost:3000/v1/events?limit=5
docker compose down -v
```

## Option 2: `docker-compose.e2e.yml` — isolated stack with stubbed RPC

```bash
docker compose -f docker-compose.e2e.yml up --build --wait
```

This is the stack the E2E test suite (`tests/e2e_tests.rs`) actually drives.
It brings up:

- **Postgres** on `localhost:5433` (tmpfs, no persistent volume)
- **WireMock** on `localhost:8080`, stubbing the Soroban RPC endpoint (mappings
  in `tests/e2e/wiremock/mappings/`)
- **the app** on `localhost:3001`, pointed at the WireMock stub
  (`STELLAR_RPC_URL=http://rpc-stub:8080`), with rate limiting disabled
  (`RATE_LIMIT_PER_MINUTE=0`)
- **a webhook receiver** on `localhost:9001` (`tests/e2e/webhook_receiver.py`)
  that records deliveries for asserting webhook behavior

By default WireMock's only mapping (`tests/e2e/wiremock/mappings/rpc_default.json`)
returns an **empty** `getEvents` result — the stack boots clean, with no
indexed events, until you either seed the database directly or make the RPC
stub return synthetic events.

### Seeding the database directly

`tests/e2e/seed.sql` inserts a known, deterministic set of rows straight into
the `events` table (bypassing the indexer entirely) — 50 `contract` events for
one contract ID, 10 `diagnostic` events for a second, and 5 `system` events
for a third, all in the ledger range 1001–1050:

```bash
psql "postgres://e2e:e2e@localhost:5433/soroban_pulse_e2e" -f tests/e2e/seed.sql
```

> The comment header in `seed.sql` references a `make e2e-seed` target; no
> such target currently exists in the `Makefile`. Run the `psql` command
> above directly until that's added.

Clear it out again with:

```bash
psql "postgres://e2e:e2e@localhost:5433/soroban_pulse_e2e" -f tests/e2e/cleanup.sql
```

After seeding, the REST API on `localhost:3001` will serve the seeded rows
immediately (no indexer wait):

```bash
curl "http://localhost:3001/v1/events/contract/CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4"
curl "http://localhost:3001/v1/events?event_type=diagnostic"
```

### Making the indexer "discover" synthetic events

If you need to exercise the indexing path itself (rather than querying
pre-seeded rows), inject a WireMock mapping so the stubbed RPC returns
synthetic Soroban events on its next `getEvents` poll. This is exactly what
the E2E suite's `stub_rpc_events` helper in `tests/e2e_tests.rs` does:

```bash
curl -X POST http://localhost:8080/__admin/mappings \
  -H "Content-Type: application/json" \
  -d '{
    "name": "getEvents-with-data",
    "priority": 1,
    "request": { "method": "POST", "url": "/", "bodyPatterns": [{"contains": "\"getEvents\""}] },
    "response": {
      "status": 200,
      "headers": {"Content-Type": "application/json"},
      "jsonBody": {
        "jsonrpc": "2.0",
        "id": 1,
        "result": { "events": [], "latestLedger": 1000 }
      }
    }
  }'
```

Replace the empty `events` array with actual Soroban RPC `getEvents` event
objects to have the indexer pick them up on its next poll. Reset stubs with:

```bash
curl -X POST http://localhost:8080/__admin/reset
```

### Testing webhook deliveries

The webhook receiver records everything posted to it; query what it's seen:

```bash
curl http://localhost:9001/received
curl -X DELETE http://localhost:9001/received   # clear recorded deliveries
```

Point a subscription's `callback_url` at `http://webhook-receiver:9001/webhook`
(from inside the compose network) to see real deliveries land there — see
[`docs/api-cookbook.md`](api-cookbook.md#1-subscribe-to-events-for-a-contract-with-a-webhook).

---

## There is no standalone fixture-generation CLI

Beyond `tests/e2e/seed.sql` / `tests/e2e/cleanup.sql` and the WireMock mapping
files, there is no dedicated tool for generating arbitrary test fixtures. If
you need data shaped differently than the seed script provides, the two
options are:

1. Copy and adapt `tests/e2e/seed.sql` (fastest for shaping specific
   `event_data` payloads, ledger ranges, or event types).
2. Run the full stack against testnet (Option 1) and use a real deployed
   Soroban contract to generate genuine events.

There is no REST endpoint for directly creating an `Event` row — events are
always indexer-derived (from RPC polling or replay), never accepted as
arbitrary user input, by design.
