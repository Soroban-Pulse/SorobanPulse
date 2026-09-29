# Historical backfill from the Stellar ledger data lake (Galexie)

Stellar RPC retains only about 7 days of events. Galexie exports `LedgerCloseMeta`
files to object storage (GCS or S3); `soroban_pulse::backfill` replays them.

## Configuration (environment)

| Variable | Description |
|---|---|
| `BACKFILL_BUCKET_URL` | HTTP(S) base URL of the bucket (public bucket or signed/proxy URL) |
| `BACKFILL_PREFIX` | Key prefix, e.g. `ledgers/pubnet` |
| `BACKFILL_START_LEDGER` / `BACKFILL_END_LEDGER` | Inclusive ledger range |
| `BACKFILL_CHUNK_SIZE` | Ledgers per range worker (default 1000) |
| `BACKFILL_WORKERS` | Parallel range workers (default 4) |
| `BACKFILL_LEDGERS_PER_FILE` | Ledgers per exported file (default 1) |

## Behaviour

- Ranges are split into chunks processed by parallel workers.
- Progress is checkpointed per chunk in `backfill_checkpoints`, so re-running resumes.
- Events use the same idempotent insert as live indexing, so overlap with live indexing is safe.
- Metric: `soroban_pulse_backfill_ledgers_total`.

## Costs and credentials

Object-store egress and request charges apply (one GET per file). Use a bucket in the
same region as the indexer. Private buckets need an authenticating proxy or signed URLs.

## Status

Range planning, object keys, checkpoints, workers and metrics are implemented. Decoding
`LedgerCloseMeta` XDR into events (`backfill::extract_events`) and the CLI subcommand wiring
are not yet implemented (requires the `stellar-xdr` dependency).

## Resource statistics (issue 1062)

Set `RESOURCE_STATS_ENABLED=true` to fetch `getTransaction` (one extra RPC call per new
event-emitting transaction, 50 per 30s) into `tx_resources`. Daily percentiles are served by
`GET /v1/contracts/{id}/resources?interval=day` from `mv_contract_resources_daily`.
