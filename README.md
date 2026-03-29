# Soroban Pulse

A lightweight Rust backend service that indexes Soroban smart contract events on the Stellar network and exposes them via a REST API.

## Tech Stack

- **Rust** + **Axum** (web framework)
- **Tokio** (async runtime)
- **PostgreSQL** + **SQLx** (database + migrations)
- **Stellar Soroban RPC** (event source)

## Project Structure

```
src/
├── main.rs       # Entry point, wires everything together
├── config.rs     # Environment config
├── db.rs         # DB pool + migrations
├── models.rs     # Data types (Event, RPC response shapes)
├── indexer.rs    # Background event polling worker
├── routes.rs     # Axum router
├── handlers.rs   # Request handlers
└── error.rs      # Unified error type
migrations/
└── 20260314000000_create_events.sql
```

## Setup

### 1. Prerequisites

- Rust (stable)
- PostgreSQL 14+
- `sqlx-cli` (optional, for manual migrations)

### 2. Configure environment

Copy the provided `.env.example` template to a new file named `.env`:

```bash
cp .env.example .env
```

Open the newly created `.env` file in your editor and fill in your own real values. Be sure to replace the placeholder credentials (e.g., `<USER>`, `<PASSWORD>`) with your actual database and network details.

| Variable          | Description                          | Default                                  |
|-------------------|--------------------------------------|------------------------------------------|
| `DATABASE_URL`    | PostgreSQL connection string         | required                                 |
| `STELLAR_RPC_URL` | Soroban RPC endpoint                 | `https://soroban-testnet.stellar.org`    |
| `DB_MAX_CONNECTIONS` | Max number of connections in the Postgres pool | `10` |
| `DB_MIN_CONNECTIONS` | Min number of connections in the Postgres pool | `1` |
| `START_LEDGER`    | Ledger to start indexing from (0 = latest) | `0`                               |
| `PORT`            | HTTP server port                     | `3000`                                   |
| `RUST_LOG`        | Log verbosity level (`trace`, `debug`, `info`, `warn`, `error`) | `info` |
| `API_KEY`         | Optional key for API authentication  | (disabled)                               |
| `RUST_LOG_FORMAT` | Log output format (`text` or `json`) | `text`                                   |
| `INDEXER_LAG_WARN_THRESHOLD` | Indexer lag warning threshold (ledgers) | `100`                                   |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry OTLP collector endpoint (when built with `otel` feature) | `http://localhost:4317` |

> **Note on Authentication:** You can enable optional API key authentication by setting the `API_KEY` environment variable. When set, all requests (except `/health` and `/healthz/*` endpoints) will require either an `Authorization: Bearer <API_KEY>` or an `X-Api-Key: <API_KEY>` header. If `API_KEY` is unset or omitted from your configuration, authentication is bypassed and all requests pass through.

### 3. Run with Docker Compose (easiest)

```bash
make docker-up
```

### 4. Run locally

```bash
# Start PostgreSQL, then:
make run
```

Migrations run automatically on startup.

### 5. Common tasks

```bash
make help   # list all available targets with descriptions
make build  # compile
make test   # run the full test suite
make lint   # clippy with warnings as errors
make fmt    # format source code
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full developer workflow.

## API

All canonical routes are versioned under `/v1/`. The unversioned paths (`/events`, etc.) remain as deprecated aliases and return a `Deprecation: true` response header.

### Interactive Documentation

- **Swagger UI**: `GET /docs` — interactive API explorer
- **OpenAPI JSON**: `GET /openapi.json` — machine-readable OpenAPI 3.0 spec

### `GET /health`
```json
{ "status": "ok" }
```

### `GET /v1/events?page=1&limit=20&exact_count=false`
Returns paginated events across all contracts.

- **`exact_count`**: (Optional) Use `true` for a precise `COUNT(*)` result on a large dataset. Default is `false`, which provides an approximate count via PostgreSQL statistics for low-latency responses.
- **`event_type`**: (Optional) Filter by event type. Accepted values: `contract`, `diagnostic`, `system`. Returns `400` for unknown values.
- **`from_ledger`**: (Optional) Return only events at or after this ledger sequence number.
- **`to_ledger`**: (Optional) Return only events at or before this ledger sequence number. Returns `400` if `from_ledger > to_ledger`.

```json
{
  "data": [
    {
      "id": "uuid",
      "contract_id": "CABC...",
      "event_type": "contract",
      "tx_hash": "abc123...",
      "ledger": 1234567,
      "timestamp": "2026-03-14T00:00:00Z",
      "event_data": { "value": {}, "topic": [] },
      "created_at": "2026-03-14T00:00:01Z"
    }
  ],
  "total": 100,
  "page": 1,
  "limit": 20,
  "approximate": true
}
```

### `GET /v1/events/{contract_id}`
Returns all events for a specific contract.

### `GET /v1/events/tx/{tx_hash}`
Returns all events from a specific transaction. If nothing has been indexed for that hash yet (including valid on-chain transactions that emitted no Soroban events), the response is **200 OK** with an empty `"data"` array — not **404**.

### `GET /v1/events/stream?contract_id=CABC...`
Server-Sent Events stream. New events are pushed to connected clients within one poll cycle of being indexed.

- **`contract_id`**: (Optional) Filter the stream to a specific contract.
- Returns `Content-Type: text/event-stream`.
- Each SSE message is a JSON-serialised event object.
- The connection is cleaned up automatically when the client disconnects.

```bash
# Subscribe to all events
curl -N http://localhost:3000/v1/events/stream

# Subscribe to a specific contract
curl -N "http://localhost:3000/v1/events/stream?contract_id=CABC..."
```

### Deprecated unversioned routes

The unversioned paths (`/events`, `/events/{contract_id}`, `/events/tx/{tx_hash}`, `/events/stream`) continue to work but return:

```
Deprecation: true
Link: </v1/events>; rel="successor-version"
```

Migrate to `/v1/` paths at your earliest convenience.

## How It Works

1. On startup, the app connects to PostgreSQL and runs migrations.
2. A background Tokio task (`indexer.rs`) polls the Soroban RPC `getEvents` method in a loop.
3. New events are inserted with `ON CONFLICT DO NOTHING` to avoid duplicates.
4. The Axum HTTP server runs concurrently, serving queries against the indexed data.

## Notes

- The indexer polls every 5 seconds when no new ledgers are available, and 10 seconds on error.
- `START_LEDGER=0` automatically starts from the latest ledger at boot time.
- All endpoints return JSON. Errors include an `"error"` field with a description.

## Observability

### Metrics

The service exposes Prometheus-compatible metrics at `GET /metrics`:

- `soroban_pulse_events_indexed_total` - Total number of events indexed
- `soroban_pulse_indexer_current_ledger` - Current ledger being processed
- `soroban_pulse_indexer_latest_ledger` - Latest ledger from RPC
- `soroban_pulse_indexer_lag_ledgers` - Lag between latest and current ledger
- `soroban_pulse_rpc_errors_total` - Total RPC errors
- `soroban_pulse_http_request_duration_seconds` - HTTP request duration by route, method, and status

### Distributed Tracing

When built with the `otel` feature, the service supports OpenTelemetry distributed tracing:

```bash
# Build with OpenTelemetry support
cargo build --features otel

# Configure the OTLP exporter endpoint
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317

# Run the service
cargo run --features otel
```

Each indexer poll cycle produces a root span with child spans for RPC and DB operations, allowing you to trace latency through the system in tools like Jaeger or Honeycomb.

### Structured Logging

Set `RUST_LOG_FORMAT=json` to output logs in JSON format for easier parsing by log aggregation tools:

```bash
export RUST_LOG_FORMAT=json
cargo run
```

## Performance

### Target SLOs

| Metric | Target |
|--------|--------|
| p99 latency (`GET /v1/events`) | < 200 ms at 100 req/s |
| Error rate | < 1% |

### Benchmarks

Criterion micro-benchmarks cover `PaginationParams::offset()` and `limit()`:

```bash
cargo bench
```

Results are written to `target/criterion/`. Run this after changes to `PaginationParams` to catch regressions. The CI pipeline runs `cargo bench` as a non-blocking step so historical results are preserved in the job logs.

### Load Testing

A [k6](https://k6.io) script targeting `GET /v1/events` lives in `tests/load/events.js`. It runs a 30-second constant-arrival-rate scenario at 100 req/s and asserts the SLOs above.

```bash
# Install k6: https://k6.io/docs/get-started/installation/
k6 run tests/load/events.js

# Point at a non-default host
k6 run -e BASE_URL=http://localhost:3000 tests/load/events.js
```

## Deployment

See [docs/deployment.md](docs/deployment.md) for TLS termination options (nginx, Caddy, AWS ALB) and production security guidance.

## Troubleshooting

**No log output after `cargo run`**
The service uses `RUST_LOG` to control log verbosity. If this variable is not set, you will see no output and may think the service is broken — it is not. Set it in your `.env` file or shell:

```bash
export RUST_LOG=info
cargo run
```

The service defaults to `info` level internally, but the environment variable must be present for the tracing subscriber to emit output. The `.env.example` file includes `RUST_LOG=info` — make sure you copied it to `.env`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, branch naming, commit conventions, and the PR process.
