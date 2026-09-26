# Query Result Streaming - Issue #960

## Overview

`src/query_streaming.rs` pulls query results out of PostgreSQL in bounded
batches instead of buffering the whole result set in memory.

This is the other half of `docs/streaming-optimization.md`. That module writes a
large response to the client without holding it all in memory, which is worth
nothing if the query itself called `fetch_all` and buffered every row before the
first byte went out. Together they give an end-to-end bound: peak memory is one
batch on the query side and a few chunks on the response side, whatever the size
of the result set.

## Why keyset cursors, not OFFSET

`OFFSET n` makes the database walk and discard `n` rows on every batch, so paging
through a large result set costs O(n²) overall. Worse, it drifts: a row inserted
behind the cursor shifts every subsequent page, so rows get skipped or repeated.

A keyset cursor carries the last key seen and asks for rows after it. Batch one
and batch ten thousand cost the same, and nothing is skipped or repeated when
rows are inserted mid-stream.

## Wiring up a query

Two pieces are needed: rows that can locate themselves, and a fetcher.

```rust
use soroban_pulse::query_streaming::{batch_fetcher, Cursored, QueryStream, StreamingQueryConfig};

impl Cursored for Event {
    fn cursor_key(&self) -> String {
        self.id.to_string()
    }
}

let fetch = batch_fetcher(move |cursor, limit| {
    let pool = pool.clone();
    async move {
        sqlx::query_as::<_, Event>(
            "SELECT * FROM events \
             WHERE ($1::text IS NULL OR id > $1) \
             ORDER BY id LIMIT $2",
        )
        .bind(cursor)
        .bind(limit)
        .fetch_all(&pool)
        .await
    }
});

let stream = QueryStream::with_config(fetch, StreamingQueryConfig::from_env());
let handle = stream.handle();

create_streaming_response(stream.into_rows())
```

The cursor key **must** be unique and ordered the same way the query orders
rows. A key that does not match the query's `ORDER BY` silently skips or repeats
rows - there is no error, just wrong output. Use the primary key, or render the
`(timestamp, id)` pair the query sorts by as one sortable string.

Keeping the fetcher a closure rather than a concrete query type is what lets the
same batching, cursor, error, keep-alive and cancellation logic sit in front of
any query in the codebase.

## Configuration

```rust
StreamingQueryConfig::default()
    .with_batch_size(1_000)
    .with_max_batches(Some(50))
    .with_keepalive_interval(Duration::from_secs(10))
    .with_error_policy(StreamErrorPolicy::SkipBatch)
    .with_max_consecutive_errors(5)
```

| Setting | Env var | Default | Meaning |
|---|---|---|---|
| `batch_size` | `QUERY_STREAM_BATCH_SIZE` | 500 | Rows per round trip, clamped to 1..=10 000 |
| `max_batches` | `QUERY_STREAM_MAX_BATCHES` | unlimited | Stop after this many batches; 0 means unlimited |
| `keepalive_interval` | `QUERY_STREAM_KEEPALIVE_SECS` | 15 | Seconds of silence before a keep-alive tick |
| `max_consecutive_errors` | `QUERY_STREAM_MAX_CONSECUTIVE_ERRORS` | 3 | Failing batches tolerated under `SkipBatch` |

`StreamingQueryConfig::from_env()` falls back to the default on an unparseable
or out-of-range value and logs it, rather than failing startup. A malformed batch
size should not take the service down.

`batch_size` is clamped rather than trusted: a batch is held in memory whole, so
an unbounded value reintroduces exactly the buffering this module exists to
avoid.

## Batch sizing

Batch size is a round-trip count against a memory ceiling.

- **Small rows, high latency to the database**: raise it. A 10-row batch over 50 000
  rows is 5 000 round trips.
- **Large rows (big `event_data` blobs)**: lower it. Memory is
  `batch_size * row_size`, and the row size is the part that surprises people.
- **Unknown**: leave it at 500. It is a reasonable middle for the event tables.

A batch that comes back shorter than `batch_size` ends the stream. The query has
no more rows, and asking again would be a round trip to learn nothing.

## Error handling

`StreamErrorPolicy` decides what a failed batch means.

### `FailFast` (default)

Surface the error to the client and end the stream. A half-delivered result set
that looks complete is worse than one that visibly failed.

### `SkipBatch`

Surface the error and retry the same cursor position, up to
`max_consecutive_errors` times in a row. For long exports where a transient blip
should not discard an hour of delivered rows.

The budget resets on any successful batch, so a stream can survive repeated
isolated failures but not a permanently broken query. Without the budget, a
query that always fails would spin forever.

Under both policies the error is yielded to the caller. Ending the stream
silently would hand back a truncated result set with nothing to distinguish it
from a complete one.

## Keep-alive

A batch that takes longer than `keepalive_interval` yields `StreamItem::KeepAlive`
rather than leaving the connection silent, so proxies and load balancers do not
reap a connection that is waiting on a legitimately slow query.

The in-flight batch is parked, not cancelled. It is moved back into the stream
state and resumed on the next poll, so a query slower than the keep-alive
interval still finishes instead of being restarted on every tick - which would
mean it never completed at all.

Transports carry the tick differently:

| Transport | Payload | Constant |
|---|---|---|
| NDJSON | blank line, skipped by every reader | `NDJSON_KEEPALIVE` |
| SSE | comment frame, ignored per the EventSource spec | `SSE_KEEPALIVE` |
| JSON array | not representable | dropped by `into_rows()` |

A JSON array body has nowhere to put a keep-alive frame, so `into_rows()` - the
adapter that feeds `streaming_response` - drops them. For a query slow enough to
need keep-alives, use NDJSON or SSE via `into_stream()`.

## Progress tracking

`QueryStream::handle()` returns a `QueryStreamHandle` before the stream is
consumed. It shares counters with the running stream:

```rust
let stats = handle.stats();
stats.avg_batch_size();  // rows per round trip
stats.is_partial();      // ended without delivering everything
```

| Field | Meaning |
|---|---|
| `batches_fetched` | Completed round trips |
| `rows_emitted` | Rows handed out |
| `keepalives_sent` | Ticks emitted during slow batches |
| `batch_errors` | Failed batches, both policies |
| `completed` | The stream ended |
| `cancelled` | It ended because someone cancelled it |
| `exhausted` | It ended because the query ran out of rows |

`is_partial()` is `completed && !exhausted`: the stream finished without
delivering the whole result set, because of `max_batches`, an error, or a
cancellation. That is the flag worth alerting on - it is the case where a client
received a plausible-looking answer that is not the whole one.

## Cancellation

`QueryStreamHandle::cancel()` stops the stream. Rows already fetched into the
current batch are dropped and no further batch is requested, which is the part
that costs. Cancelling before the first poll means no query runs at all.

A client that disconnects cancels through the response layer: the body is
dropped, `streaming_response` stops pulling, and this stream stops being polled.

## Metrics

| Metric | Type | Meaning |
|---|---|---|
| `soroban_pulse_query_stream_rows_total` | counter | Rows handed out |
| `soroban_pulse_query_stream_batches_total` | counter | Completed batch fetches |
| `soroban_pulse_query_stream_batch_rows` | histogram | Rows per batch |
| `soroban_pulse_query_stream_batch_errors_total` | counter | Failed batches |
| `soroban_pulse_query_stream_keepalives_total` | counter | Keep-alive ticks |
| `soroban_pulse_query_streams_cancelled_total` | counter | Streams stopped by a caller |
| `soroban_pulse_query_streams_truncated_total` | counter | Streams stopped at `max_batches` |
| `soroban_pulse_query_streams_completed_total` | counter | Streams that ran to exhaustion |
| `soroban_pulse_query_stream_rows_per_stream` | histogram | Rows per completed stream |

### Reading them

- **`batch_rows` well below `batch_size`**: the query's filters are more
  selective than the batch size assumes. Round trips are being spent on nearly
  empty batches.
- **`keepalives_total` rising**: batches are taking longer than the interval.
  Check the query plan before raising the interval.
- **`truncated_total` rising**: clients are hitting `max_batches`. Either the
  limit is too low or the queries are too broad.
- **`batch_errors_total` rising with `completed_total` flat**: streams are
  failing rather than finishing.

## Related

- Writing the rows to the client: `docs/streaming-optimization.md`
- Reusing serialized payloads: `docs/serialization-caching.md`
