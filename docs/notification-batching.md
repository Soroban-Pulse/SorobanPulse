# Notification Batching

To reduce per-message overhead (connection setup, TLS handshakes, downstream
rate limits) and improve overall throughput, outbound notifications can be
grouped into batches instead of being delivered one at a time. This is
implemented in `src/notification_batching.rs`.

## Configuration

```rust
use sorobanpulse::notification_batching::BatchConfig;
use std::time::Duration;

let config = BatchConfig {
    enabled: true,
    max_batch_size: 50,
    max_batch_window: Duration::from_secs(5),
    max_concurrent_batches: 10,
    dedup_enabled: true,
};
```

| Field | Description |
|---|---|
| `enabled` | Master switch; when `false`, notifications flush immediately (no batching). |
| `max_batch_size` | Batch flushes as soon as it reaches this many notifications. |
| `max_batch_window` | Batch flushes after this much time even if under size threshold. |
| `max_concurrent_batches` | Caps how many batches may be in flight to the downstream sender at once. |
| `dedup_enabled` | When `true`, notifications sharing a `dedup_key` within the same batch window are dropped. |

## Batch Size Thresholds & Time Windows

`NotificationBatcher::add` accumulates notifications and returns
`Some(FlushedBatch)` as soon as `max_batch_size` is reached
(`FlushReason::SizeThreshold`). A caller should also periodically invoke
`poll_time_window()` (e.g. on a tick every second) so batches that never hit
the size threshold still flush once `max_batch_window` elapses
(`FlushReason::TimeWindow`). `flush_now()` drains any pending notifications
immediately, e.g. during graceful shutdown.

## Deduplication

When `dedup_enabled` is set, each notification may carry a `dedup_key`. The
batcher keeps a `HashSet` of keys seen in the current batch window; a second
notification with the same key is dropped and counted in
`BatchMetrics::duplicates_dropped` rather than being delivered twice.

## Error Handling

`handle_batch_error` inspects a `BatchDeliveryError` and the batch that failed
to decide the next action:

- **Timeout with more than one notification** → `SplitAndRetry`: break the
  batch in half and retry each half, since a smaller batch is more likely to
  fit within the downstream timeout.
- **Concurrency limit exceeded** → `RequeueLater`: back off and try again
  once a slot frees up.
- **Otherwise** → `RetryWhole`: retry the batch as-is.

Every failure increments `BatchMetrics::delivery_failures`.

## Metrics

`BatchMetrics` tracks:

- `notifications_enqueued`
- `notifications_delivered`
- `batches_flushed`
- `duplicates_dropped`
- `delivery_failures`
- `average_batch_size()` — delivered notifications divided by batches flushed

These are intended to be exported alongside the existing Prometheus metrics
in `src/metrics.rs`.
