# Streaming Response Optimization - Issue #958

## Overview

`src/streaming_response.rs` writes large JSON result sets to clients without
ever holding the whole response in memory. Rows are serialized one at a time,
batched into chunks, and pushed to the HTTP layer as they are produced.

Peak memory for a response is bounded by:

```
buffer_size * (channel_capacity + 1)
```

With the defaults that is roughly 40 KB regardless of whether the query returns
ten rows or ten million.

## Architecture

```
sqlx stream ──> producer task ──> bounded channel ──> axum Body ──> client
                     │                   │
                serialize +          backpressure
                chunk + encode       (blocks producer)
```

The producer runs on its own task. It is joined to the HTTP writer by a bounded
`tokio::sync::mpsc` channel, and that bound is the whole design: when the client
reads slowly the channel fills, the producer parks, and the database stream
stops being polled. An unbounded channel would let the entire result set
accumulate in memory, which is exactly what streaming was supposed to prevent.

## Configuration

`StreamingOptions` is a builder; every field has a default that suits a normal
endpoint.

| Option | Default | Effect |
|---|---|---|
| `buffer_size` | 8192 bytes | Bytes accumulated before a chunk is flushed. Larger means fewer, bigger writes. |
| `channel_capacity` | 4 chunks | Chunks in flight. Larger smooths jittery clients; smaller applies backpressure sooner. |
| `compression` | `None` | `Gzip` compresses each chunk as it goes past. |

```rust
use soroban_pulse::streaming_response::{StreamingJsonResponse, StreamingOptions, StreamCompression};

let options = StreamingOptions::default()
    .with_buffer_size(16_384)
    .with_channel_capacity(8)
    .with_compression(StreamCompression::Gzip);

StreamingJsonResponse::with_options(row_stream, options)
```

Both `buffer_size` and `channel_capacity` are clamped to a minimum of 1. A zero
channel capacity panics inside tokio, and a zero buffer would flush per item,
so neither is taken at face value.

## Using it from an endpoint

`create_negotiated_streaming_response` is the entry point handlers should use.
It reads the request's `Accept-Encoding` and picks the encoding, so every
endpoint streams the same way instead of each one choosing its own chunk size
and encoding rules:

```rust
let accept = headers
    .get(axum::http::header::ACCEPT_ENCODING)
    .and_then(|v| v.to_str().ok());

create_negotiated_streaming_response(row_stream, accept)
```

Anything the module cannot encode falls back to identity, which is always a
valid response, so an unusual `Accept-Encoding` can never fail a request.

## Chunked encoding

The response sets `Transfer-Encoding: chunked` and `Cache-Control: no-store`.
The body length is not known when the headers go out, so a cache or proxy must
not treat a truncated body as a complete one.

Items are coalesced into `buffer_size` chunks rather than sent individually. A
million-row response costs a million serializations either way, but batching
avoids a million channel sends and task wakeups.

## Compression

`StreamCompression::Gzip` compresses each chunk with a sync flush at the chunk
boundary. That flush is what keeps the response streaming: without it the
encoder would hold everything until the stream ended, and the client would wait
for the last row before seeing the first.

The trade is a slightly worse ratio than compressing the whole body at once.
That is the intended trade - a streaming response that has to be fully buffered
to compress is not a streaming response.

`StreamingStats::compression_ratio()` reports encoded size over raw size, and is
1.0 for an uncompressed stream.

## Backpressure

`send_chunk` tries a non-blocking send first, so a client that keeps up costs
nothing extra. When the channel is full it records a backpressure event and then
awaits the send, which is where the producer actually parks.

`soroban_pulse_streaming_response_backpressure_total` rising is not a fault. It
means backpressure is working. It is, however, the early warning that clients
are reading slower than the database produces and that request timeouts are
coming.

## Progress tracking

`StreamingJsonResponse::handle()` returns a `StreamHandle` before the response is
converted into a body. It shares counters with the producer, so progress is
observable while the response is still being written:

```rust
let response = StreamingJsonResponse::new(row_stream);
let handle = response.handle();

// elsewhere, while the response is still streaming
let stats = handle.stats();
tracing::info!(items = stats.items_sent, chunks = stats.chunks_sent, "in flight");
```

`StreamingStats` carries `items_sent`, `chunks_sent`, `bytes_before_encoding`,
`bytes_after_encoding`, `errors` split into `serialization_errors` and
`database_errors`, `backpressure_waits`, `completed`, and `cancelled`.

## Cancellation

Two things stop a stream:

- **The caller** calls `StreamHandle::cancel()`. The producer stops after the
  item it is on and the stats report `cancelled`.
- **The client disconnects.** The channel receiver drops, sends start failing,
  and the producer gives up rather than draining a result set nobody is reading.

Cancellation is not instantaneous - the producer finishes the current item
first - but it does stop the database stream from being polled any further,
which is the part that costs.

## Error handling

A row that fails to serialize, or a database error mid-stream, is recorded and
skipped. The response is already partly written by then, so there is no status
code left to change; aborting would hand the client a truncated document with no
explanation. Skipping keeps the array well-formed and the failure visible in
metrics and in `StreamingStats`.

## Metrics

| Metric | Type | Meaning |
|---|---|---|
| `soroban_pulse_streaming_response_items_sent_total` | counter | Rows written across all streams |
| `soroban_pulse_streaming_responses_completed_total` | counter | Streams that ran to completion |
| `soroban_pulse_streaming_response_items_per_stream` | histogram | Rows per response |
| `soroban_pulse_streaming_response_errors_total` | counter | Labelled `serialization`, `database`, `compression` |
| `soroban_pulse_streaming_response_chunks_total` | counter | Chunks flushed |
| `soroban_pulse_streaming_response_chunk_bytes` | histogram | Bytes per chunk on the wire |
| `soroban_pulse_streaming_response_backpressure_total` | counter | Times a producer parked on a full channel |
| `soroban_pulse_streaming_responses_cancelled_total` | counter | Labelled `caller` or `client` |
| `soroban_pulse_streaming_response_duration_seconds` | histogram | Wall time per response |

## Tuning guidance

- **Large rows, fast client**: raise `buffer_size` to cut syscalls.
- **Many slow clients**: lower `channel_capacity` so memory per in-flight
  request stays small, and accept more backpressure events.
- **Bandwidth-constrained clients**: enable gzip. CPU cost is per chunk and
  scales with the response, so watch process CPU when turning it on broadly.
- **Rising `backpressure_total` with rising `duration_seconds`**: clients cannot
  keep up. Raising the buffer will not help; the fix is a smaller result set
  (narrower query, keyset paging) or a faster client.

## Related

- Query-side batching and cursors: `docs/query-streaming.md`
- Serialized-payload reuse: `docs/serialization-caching.md`
