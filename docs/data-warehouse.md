# Data Warehouse Export

SorobanPulse can export event data to external analytics warehouses
(BigQuery and Snowflake) in addition to local/S3 Parquet files produced by
`src/parquet_export.rs`.

## Overview

The `src/warehouse/` module provides:

- `bigquery.rs` — client for BigQuery's `tabledata.insertAll` streaming API.
- `snowflake.rs` — client for Snowflake's SQL REST API (`/api/v2/statements`).
- `schema_mapping.rs` — canonical event schema mapped to BigQuery and
  Snowflake native column types.
- `incremental.rs` — watermark-based incremental load support, so repeated
  exports only ship rows newer than the last successfully exported
  `ingested_at` value.
- `transform.rs` — default row transformations (drops internal `_` fields,
  flattens nested `topics` arrays, normalizes timestamps to RFC3339 UTC).

## Configuration

```rust
use soroban_pulse::warehouse::{WarehouseExportConfig, WarehouseKind};

let config = WarehouseExportConfig {
    kind: WarehouseKind::BigQuery,
    dataset_or_schema: "soroban_pulse".into(),
    table: "events".into(),
    incremental: true,
    watermark_column: "ingested_at".into(),
    batch_size: 5_000,
};
```

Environment variables:

| Variable | Purpose |
|---|---|
| `BIGQUERY_PROJECT_ID` | GCP project hosting the destination dataset |
| `BIGQUERY_SERVICE_ACCOUNT_JSON` | Path to a service-account key used to mint OAuth2 access tokens |
| `SNOWFLAKE_ACCOUNT` | Snowflake account identifier (e.g. `xy12345.us-east-1`) |
| `SNOWFLAKE_WAREHOUSE` | Virtual warehouse used to run load statements |
| `SNOWFLAKE_AUTH_TOKEN` | OAuth or key-pair JWT for the REST API |

## Usage

```rust
use soroban_pulse::warehouse::{export_to_warehouse, bigquery::BigQueryClient};

let client = BigQueryClient::new(project_id, access_token);
let result = export_to_warehouse(&client, &config, &rows)?;
println!("exported {} rows in {} batches", result.rows_exported, result.batches);
```

## Incremental loads

When `config.incremental` is `true`, `export_to_warehouse` filters the
input batch down to rows whose `watermark_column` value is greater than the
last recorded watermark for that `(warehouse, dataset, table)` triple, then
advances the watermark after a successful export. This makes repeated,
scheduled export runs (e.g. every 5 minutes via a cron job) safe to re-run
without duplicating rows already loaded.

## Schema mapping

`schema_mapping::event_columns()` is the single source of truth for column
names/types across both warehouses:

| Column | BigQuery | Snowflake |
|---|---|---|
| event_id | STRING | VARCHAR |
| ledger_sequence | INT64 | NUMBER |
| contract_id | STRING | VARCHAR |
| event_type | STRING | VARCHAR |
| topics | JSON | VARIANT |
| payload | JSON | VARIANT |
| ingested_at | TIMESTAMP | TIMESTAMP_NTZ |
| tx_hash | STRING | VARCHAR |

## Testing

Unit tests for the row-transformation logic live in
`src/warehouse/transform.rs` (`cargo test warehouse::transform`). Warehouse
API clients are structured so their HTTP calls can be mocked/injected in
integration tests by implementing the `WarehouseExporter` trait.

## Limitations / follow-ups

- The BigQuery and Snowflake clients build request payloads but the actual
  HTTP transport call is left as an integration point (`_body`/`_stmt` in
  `bigquery.rs`/`snowflake.rs`) so this can be wired to the project's
  existing HTTP client and retry/backoff middleware.
- `WatermarkStore` is currently in-memory; production use should persist
  watermarks in the existing Postgres database.
