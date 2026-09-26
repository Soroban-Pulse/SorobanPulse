# HTTP Response Compression

_Issue #961_

Soroban Pulse compresses HTTP responses in-flight using
[`tower_http::compression::CompressionLayer`](https://docs.rs/tower-http/latest/tower_http/compression/index.html),
negotiated per-request via the client's `Accept-Encoding` header (gzip,
deflate, or br). Configuration lives in
[`src/compression_config.rs`](../src/compression_config.rs).

## Configuration

| Env var | Default | Meaning |
|---|---|---|
| `COMPRESSION_LEVEL` | `6` | Quality 1 (fastest, lowest ratio) – 9 (slowest, highest ratio). Values outside `1..=9` are clamped. |
| `COMPRESSION_MIN_SIZE_BYTES` | `256` | Responses smaller than this many bytes (per `Content-Length`) skip compression entirely. |

Both are read once at startup in
[`routes::build_router`](../src/routes.rs) via
`CompressionSettings::from_env()`. There is no hot-reload — restart the
process to pick up a change.

## Why a size floor

Gzip has its own header/trailer overhead (~20 bytes) and a non-trivial
per-call CPU cost. For a tiny response — a health check, an empty `{"data":
[]}` page — compressing it can produce a *larger* payload while still
spending CPU. `COMPRESSION_MIN_SIZE_BYTES` lets an operator tune where that
tradeoff sits for their traffic mix; the default of 256 bytes is safely
above gzip's own overhead.

## What never gets compressed, regardless of size

The size floor is combined with tower-http's `DefaultPredicate`, which
additionally excludes:

- `text/event-stream` (the `/v1/events/feed` SSE endpoint) — compressing a
  streaming response would buffer chunks and break the "arrives as it's
  produced" contract.
- `application/grpc` and other content types where compression is either
  redundant (already-compressed media) or actively wrong.

See `CompressionSettings::predicate` for the exact composition
(`SizeAbove::new(min_size_bytes).and(DefaultPredicate::new())`).

## Metrics

Every response that passes through the compression layer increments one of:

- `soroban_pulse_http_compression_applied_total` — response left the layer
  with a `Content-Encoding` header set.
- `soroban_pulse_http_compression_bypassed_total` — response was left
  untouched (too small, excluded content type, or the client didn't
  advertise a supported encoding).

These are distinct from `soroban_pulse_compression_ratio` /
`soroban_pulse_events_compressed_total`, which track storage-level event
archival compression (see `src/event_compression.rs`), not the HTTP
transport layer.

A low `applied` / `bypassed` ratio in production is a signal to lower
`COMPRESSION_MIN_SIZE_BYTES`; a saturated CPU with compression dominating
flame graphs is a signal to lower `COMPRESSION_LEVEL`.

## Benchmarking

`benches/compression.rs` measures gzip ratio and wall-clock time at 10/100/
1000-event response sizes:

```bash
cargo bench --bench compression
```
