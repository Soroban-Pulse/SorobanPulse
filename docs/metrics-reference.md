# Operational Metrics Reference (Issue #918)

Soroban Pulse exposes ~150 Prometheus metrics from `GET /metrics`, all
prefixed `soroban_pulse_`. This document catalogs every metric emitted by the
codebase: what it means, its type and unit, when to alert on it, how metrics
relate to each other, and where they show up in the shipped Grafana
dashboards. All names below are grep'd directly from `src/metrics.rs` and the
handful of source files that emit metrics inline (`src/replica_monitor.rs`,
`src/index_monitor.rs`, `src/partition_manager.rs`, `src/slo_tracker.rs`,
`src/stats_refresh.rs`, `src/statistics_management.rs`,
`src/feature_flags.rs`, `src/alert_manager.rs`,
`src/distributed_tracing.rs`) — if a metric here doesn't match what you see
on `/metrics`, the source has moved on; `grep -rn '"<metric name>"' src/` is
the ground truth.

## How to read this reference

- **Type** — `Counter` (monotonically increasing, use `rate()`/`increase()`),
  `Gauge` (point-in-time value, can go up or down), `Histogram` (bucketed
  distribution, use `histogram_quantile()`).
- **Labels** — Prometheus label dimensions attached to the series. High-label
  metrics are noted with a cardinality caution.
- **Unit** is stated in the description; Counters ending `_total` count
  events, `_seconds`/`_ms`/`_us` are durations, `_bytes` are byte counts,
  ratios/scores are unitless `[0, 1]` or `[0, 100]` as noted.

---

## Indexer & ledger progress

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_indexer_current_ledger` | Gauge | — | Ledger sequence the indexer has processed up to. |
| `soroban_pulse_indexer_latest_ledger` | Gauge | — | Latest ledger reported by the Stellar RPC endpoint. |
| `soroban_pulse_indexer_lag_ledgers` | Gauge | — | `latest_ledger − current_ledger`. The primary indexer-health signal. |
| `soroban_pulse_indexer_lag_observation_ledgers` | Histogram | — | Distribution of lag samples over time, feeds the Grafana lag heatmap. |
| `soroban_pulse_indexer_checkpoint_ledger` | Gauge | — | Last ledger checkpointed to durable storage (recovery resume point). |
| `soroban_pulse_indexer_is_leader` | Gauge | — | `1.0` if this replica holds the advisory lock and is actively indexing, `0.0` otherwise. In a multi-replica deployment, exactly one instance should report `1.0`. |
| `soroban_pulse_events_indexed_total` | Counter | — | Events successfully written to the `events` table. |
| `soroban_pulse_contract_event_count` | Gauge | `contract_id` | Per-contract event count, used for the "Contract Popularity Top-10" panel. **Cardinality caution**: one series per distinct contract ID ever seen. |

## Event validation & processing

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_events_validation_failed_total` | Counter | — | Events that failed general validation. |
| `soroban_pulse_events_oversized_total` | Counter | — | Events skipped for exceeding `MAX_EVENT_DATA_BYTES`. |
| `soroban_pulse_events_duplicate_total` | Counter | — | Events rejected as duplicates. |
| `soroban_pulse_events_xdr_invalid_total` | Counter | — | Events with unparseable XDR payloads. |
| `soroban_pulse_events_invalid_contract_id_total` | Counter | — | Events with a malformed contract ID. |
| `soroban_pulse_xdr_validation_pass_total` | Counter | — | XDR validations that passed. |
| `soroban_pulse_xdr_validation_fail_total` | Counter | `field` | XDR validations that failed, broken down by which field failed. |
| `soroban_pulse_normalizer_errors_total` | Counter | — | Errors in the event data normalization step. |
| `soroban_pulse_bloom_filter_hits_total` | Counter | — | Pre-filter dedup hits from the ledger-scoped bloom filter. |
| `soroban_pulse_bloom_filter_size` | Gauge | — | Number of set bits in the bloom filter. |
| `soroban_pulse_session_bloom_hits_total` | Counter | — | Dedup hits from the per-session bloom filter. |
| `soroban_pulse_session_bloom_resets_total` | Counter | — | Session bloom filter resets (fires each new ledger). |
| `soroban_pulse_content_dedup_hits_total` | Counter | — | Events skipped because their content fingerprint matched a recent event (content-identical retry). |
| `soroban_pulse_fingerprints_stored_total` | Counter | — | Content fingerprints computed and stored. |
| `soroban_pulse_schema_validation_pass_total` | Counter | `contract_id` | Contract-schema validations that passed. |
| `soroban_pulse_schema_validation_fail_total` | Counter | `contract_id` | Contract-schema validations that failed. |

## RPC & network

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_rpc_errors_total` | Counter | — | RPC call failures (timeout, connection refused, non-2xx, unparseable body). |
| `soroban_pulse_rpc_failover_total` | Counter | — | Times the indexer switched from the primary RPC URL to a fallback. |
| `soroban_pulse_rpc_active_endpoint` | Gauge | `url` | Set to `1.0` for whichever RPC URL is currently active; useful for confirming failover took effect. |
| `soroban_pulse_rpc_health_checks_total` | Counter | `status` (`ok`\|`error`) | RPC health-check outcomes. |
| `soroban_pulse_rpc_health_check_duration_ms` | Histogram | — | RPC health-check round-trip time. |
| `soroban_pulse_network_healthy` | Gauge | `chain_id` | `1.0`/`0.0` per-chain health for multi-chain deployments. |
| `soroban_pulse_network_latest_ledger` | Gauge | `chain_id` | Latest ledger per chain in a multi-chain deployment. |
| `soroban_pulse_network_indexer_errors_total` | Counter | `chain_id` | Indexer errors, broken out per chain. |

## Database & connection pool

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_db_pool_size` | Gauge | — | Current total connections held by the pool (idle + active). |
| `soroban_pulse_db_pool_idle` | Gauge | — | Idle connections in the pool. |
| `soroban_pulse_db_pool_active_connections` | Gauge | — | `size − idle`; connections currently checked out. |
| `soroban_pulse_db_pool_max_connections` | Gauge | — | Configured `DB_MAX_CONNECTIONS` ceiling, mirrored as a metric for ratio math in dashboards. |
| `soroban_pulse_db_pool_utilization` | Gauge | — | `active / max`, range `[0, 1]`. The number to alert on. |
| `soroban_pulse_db_pool_acquire_latency_seconds` | Histogram | — | Time spent waiting to check out a connection. Rising values precede exhaustion. |
| `soroban_pulse_db_pool_exhaustion_alerts_total` | Counter | — | Times utilization crossed the exhaustion threshold (default 90%, see `src/connection_pool.rs`). |
| `soroban_pulse_slow_queries_total` | Counter | `query_type` | Queries exceeding the slow-query threshold (issue #421). |
| `soroban_pulse_query_duration_seconds` | Histogram | `query_type` | Query duration by logical query type. |
| `soroban_pulse_stats_last_analyzed_age_seconds` | Gauge | — | Seconds since `ANALYZE` last ran on tracked tables; large values mean the planner is working off stale statistics. |
| `soroban_pulse_stale_tables_total` | Gauge | — | Count of tables whose statistics are considered stale. |
| `soroban_pulse_statistics_health_score` | Gauge | — | Composite `0–100` score for planner-statistics freshness. |
| `soroban_pulse_migrations_applied_total` | Counter | — | Migrations applied during this run. |
| `soroban_pulse_last_migration_version` | Gauge | — | Highest applied migration version. |

## Query caching & planning

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_query_cache_hits_total` | Counter | `query_type` | Query-result cache hits. |
| `soroban_pulse_query_cache_misses_total` | Counter | `query_type` | Query-result cache misses. |
| `soroban_pulse_query_plan_estimated_rows` | Histogram | `query_type` (or `query` from `statistics_management.rs`) | Planner's estimated row count from `EXPLAIN`, used to spot cardinality-estimate drift. |
| `soroban_pulse_query_plan_cache_hits_total` | Counter | — | Prepared-statement plan cache hits. |
| `soroban_pulse_query_plan_cache_misses_total` | Counter | — | Plan cache misses (triggers a fresh `EXPLAIN`/plan). |
| `soroban_pulse_query_plans_cached_total` | Counter | — | Plans inserted into the cache. |
| `soroban_pulse_query_plan_cache_evictions_total` | Counter | — | LRU/TTL evictions from the plan cache. |
| `soroban_pulse_query_plan_cache_hit_ratio` | Gauge | — | `[0, 1]`, refreshed on every cache `get()`. |
| `soroban_pulse_query_plan_cache_entry_count` | Gauge | — | Live entries in the plan cache. |
| `soroban_pulse_query_planning_time_ms` | Histogram | — | Time spent in the Postgres planner. |
| `soroban_pulse_serialization_cache_hits_total` / `_misses_total` | Counter | `entity_type` | JSON serialization cache hit/miss (issue #687). |
| `soroban_pulse_serialization_time_us` | Histogram | `entity_type` | JSON serialization time, microseconds. |
| `soroban_pulse_search_query_duration_seconds` | Histogram | — | Full-text search query duration. |
| `soroban_pulse_fulltext_searches_total` | Counter | — | Full-text search queries executed. |
| `soroban_pulse_aggregation_queries_total` | Counter | — | Aggregation queries executed. |
| `soroban_pulse_timeseries_query_duration_seconds` | Histogram | — | Time-series query duration. |
| `soroban_pulse_temporal_query_duration_seconds` | Histogram | — | Point-in-time/temporal query duration (issue #581). |
| `soroban_pulse_contract_history_query_duration_seconds` | Histogram | — | Contract-history query duration. |
| `soroban_pulse_batch_queries_total` / `batch_query_events_total` | Counter | — | Batch query count and total events returned by batch queries (issue #624). |
| `soroban_pulse_archive_queries_total` / `archive_restored_events_total` | Counter | — | Cold-storage archive query count and events restored (issue #623). |
| `soroban_pulse_archive_integrity_failures_total` | Counter | — | Archive integrity check failures (issue #371). |
| `soroban_pulse_contract_count_cache_hit_ratio` | Gauge | — | Hit ratio for the per-contract count cache. |
| `soroban_pulse_contract_count_cache_invalidations_total` | Counter | — | Cache invalidations for the same. |

## Replication (read replicas)

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_replica_count` | Gauge | — | Number of read replicas currently discovered/monitored. `0` means no replicas are visible — alerts on this. |
| `soroban_pulse_replica_lag_bytes` | Gauge | `client_addr` (plus an `"aggregate"` series) | WAL replication lag in bytes per replica. |
| `soroban_pulse_replica_replay_lag_seconds` | Gauge | `client_addr` (plus `"aggregate"`) | Replay lag in seconds — the number the `ReplicaLagHigh`/`ReplicaLagCritical` alerts key on. |
| `soroban_pulse_replica_write_lag_seconds` | Gauge | `client_addr` | Time for WAL to reach the replica's OS. |
| `soroban_pulse_replica_flush_lag_seconds` | Gauge | `client_addr` | Time for WAL to be flushed to replica disk. |
| `soroban_pulse_replica_slot_lag_bytes` | Gauge | `client_addr` | Replication slot retention lag — large values risk WAL bloat on the primary. |
| `soroban_pulse_cascade_replica_depth` | Gauge | `client_addr` | Depth in a cascading replication topology. |
| `soroban_pulse_replica_health_score` | Gauge | — | `0` or `100` composite health signal (see `src/replica_monitor.rs`). |
| `soroban_pulse_failover_events_total` | Counter | — | Replica failover events. |

## Partitioning & schema health

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_partition_count` | Gauge | — | Total partitions on the partitioned events table. |
| `soroban_pulse_partition_created_total` | Counter | — | New partitions created. |
| `soroban_pulse_partition_total_size_bytes` | Gauge | — | Combined size of all partitions. |
| `soroban_pulse_partition_pruning_effectiveness` | Gauge | — | `0–100`, how effectively partition pruning is eliminating scans. |
| `soroban_pulse_partition_skew_max` | Gauge | — | Largest size skew observed across partitions — high skew means uneven ledger distribution. |
| `soroban_pulse_hot_partitions_count` | Gauge | — | Partitions receiving disproportionate scan activity. |
| `soroban_pulse_archived_partitions_total` | Counter | — | Partitions moved to cold storage. |
| `soroban_pulse_ledger_partitions_total` / `_active` / `_archived` | Gauge | — | Ledger-range partition counts by state. |
| `soroban_pulse_ledger_partition_created_total` | Counter | — | Ledger partitions created. |
| `soroban_pulse_ledger_partition_total_size_bytes` | Gauge | — | Size of ledger-range partitions. |
| `soroban_pulse_schema_missing_future_partitions` | Gauge | — | Missing future-month partitions for the events table. `> 0` means pre-creation is falling behind (issue #804). |
| `soroban_pulse_schema_unused_indexes_total` | Gauge | — | Indexes with zero scans since the last stats reset, **excluding** newly-created partition indexes (issue #804 schema-health scope). |
| `soroban_pulse_unused_indexes_total` | Gauge | — | A broader unused-index count from `src/index_monitor.rs` (no partition-index exclusion). This is the metric the `UnusedIndexesDetected` alert in `docs/alerts.yml` actually reads — see [Known discrepancies](#known-discrepancies-between-docs-and-code). |
| `soroban_pulse_fragmented_indexes_total` | Gauge | — | Indexes flagged as bloated/fragmented. |
| `soroban_pulse_index_scan_count` | Gauge | `table`, `index` | Per-index scan counts, mirrors `pg_stat_user_indexes.idx_scan`. **Cardinality caution**: one series per index. |
| `soroban_pulse_index_bloat_ratio` | Gauge | `table`, `index` | Estimated bloat ratio per index. |
| `soroban_pulse_index_size_bytes` | Gauge | `table`, `index` | Per-index size. |
| `soroban_pulse_index_dead_tuples` | Gauge | `table`, `index` | Dead tuples behind the index, when known. |
| `soroban_pulse_matview_refresh_duration_seconds` | Histogram | `view` | Materialized view refresh time. |
| `soroban_pulse_matview_refresh_timeout_total` | Counter | `view` | Refreshes that hit a lock timeout instead of completing. |
| `soroban_pulse_events_pruned_total` | Counter | — | Events pruned by retention policy. |
| `soroban_pulse_events_deleted_total` | Counter | — | Events deleted via GDPR right-to-erasure requests. |

## HTTP & API layer

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_http_request_duration_seconds` | Histogram | `method`, `route`, `status` | End-to-end request duration. Custom SLO-aligned buckets: `[0.05, 0.1, 0.2, 0.5, 1.0, 5.0]`s (set in `init_metrics()`); every other histogram in this catalog uses the exporter's default buckets. **Cardinality caution**: `route` should be the templated path (`/v1/events`), not the raw URI — confirm this if adding new routes. |
| `soroban_pulse_rate_limit_rejected_total` | Counter | — | Requests rejected with `429`. |
| `soroban_pulse_conditional_get_304_total` | Counter | — | `304 Not Modified` responses served (issue #885). |
| `soroban_pulse_conditional_get_bandwidth_saved_bytes_total` | Counter | — | Estimated bytes saved by conditional GETs. |
| `soroban_pulse_etag_cache_hits_total` / `_misses_total` | Counter | — | ETag cache hit/miss for conditional requests. |
| `soroban_pulse_streaming_response_items_sent_total` | Counter | — | Items sent via streaming (NDJSON) responses (issue #688). |
| `soroban_pulse_streaming_responses_completed_total` | Counter | — | Streaming responses completed. |
| `soroban_pulse_streaming_response_items_per_stream` | Histogram | — | Distribution of item counts per completed stream. |
| `soroban_pulse_streaming_response_errors_total` | Counter | `error_type` | Streaming response errors by type. |
| `soroban_pulse_compression_ratio` | Histogram | — | `compressed_bytes / original_bytes` per compressed payload. |
| `soroban_pulse_events_compressed_total` | Counter | — | Payloads compressed. |
| `soroban_pulse_compression_bytes_saved_total` | Counter | — | Cumulative bytes saved by compression. |
| `soroban_pulse_decompression_failures_total` | Counter | — | Decompression failures. |

## SSE & WebSocket streaming

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_sse_active_connections` | Gauge | — | Currently open SSE connections. Note: some panels in `docs/grafana-dashboard.json` reference `soroban_pulse_sse_connections_active` (reversed word order) — see [Known discrepancies](#known-discrepancies-between-docs-and-code). |
| `soroban_pulse_ws_active_connections` | Gauge | — | Currently open WebSocket connections. |
| `soroban_pulse_sse_connections_per_ip` | Histogram | — | Per-IP SSE connection count distribution (issue #453, abuse detection). |
| `soroban_pulse_sse_multi_contract_ids` | Histogram | — | Number of contract IDs subscribed per multi-contract SSE stream. |
| `soroban_pulse_sse_lagged_events_total` | Counter | `connection_id` | Events a slow SSE client missed because it fell behind. **Cardinality caution**: unbounded by connection ID — treat as high-cardinality and avoid long-term retention of this series by ID; aggregate at query time. |
| `soroban_pulse_sse_replayed_events_total` | Counter | — | Events replayed to a reconnecting client via `Last-Event-ID`. |
| `soroban_pulse_sse_ring_buffer_size` | Gauge | — | Current size of the SSE replay ring buffer. |
| `soroban_pulse_sse_ring_buffer_overflows_total` | Counter | — | Ring buffer overflows (oldest event evicted). |
| `soroban_pulse_sse_ring_buffer_misses_total` | Counter | — | Reconnects whose `Last-Event-ID` had already been evicted, falling back to a DB replay. |

## Notifications & delivery channels

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_notification_delivery_success_total` / `_failure_total` | Counter | — | Aggregate notification delivery outcomes across all channels (issue #695). |
| `soroban_pulse_notification_delivery_latency_seconds` | Histogram | `channel` | Delivery latency per channel (issue #513). |
| `soroban_pulse_notification_rate_per_minute` | Gauge | `channel` | Current delivery rate per channel (issue #514). |
| `soroban_pulse_notification_channel_healthy` | Gauge | `channel`, `type` | `1.0`/`0.0` health per configured channel (issue #498). |
| `soroban_pulse_notifications_maintenance_suppressed_total` | Counter | — | Notifications suppressed by an active maintenance window (issue #495). |
| `soroban_pulse_webhook_failures_total` | Counter | — | Webhook deliveries that exhausted retries. |
| `soroban_pulse_webhook_delivery_success_total` | Counter | — | Successful webhook deliveries. |
| `soroban_pulse_pagerduty_failures_total` | Counter | — | PagerDuty delivery failures. |
| `soroban_pulse_github_failures_total` | Counter | — | GitHub-integration delivery failures. |
| `soroban_pulse_discord_failures_total` | Counter | — | Discord delivery failures. |
| `soroban_pulse_slack_failures_total` | Counter | — | Slack delivery failures. |
| `soroban_pulse_telegram_failures_total` | Counter | — | Telegram delivery failures. |
| `soroban_pulse_email_failures_total` | Counter | — | Email delivery failures. |
| `soroban_pulse_email_bounces_total` | Counter | — | Bounces reported via the bounce webhook (issue #484). |
| `soroban_pulse_subscription_email_sent_total` | Counter | — | Subscription-triggered emails sent (issue #619). |
| `soroban_pulse_subscription_email_rate_limited_total` | Counter | — | Emails suppressed by the per-subscriber daily cap. |
| `soroban_pulse_subscription_email_config_updates_total` | Counter | — | Subscription email config changes. |
| `soroban_pulse_push_sent_total` / `push_failed_total` | Counter | `device_type` | Mobile push delivery outcomes (issue #620). |
| `soroban_pulse_push_token_invalid_total` | Counter | — | Invalid/expired push tokens cleaned up. |
| `soroban_pulse_push_retries_total` | Counter | `device_type`, `attempt` | Push delivery retry attempts (issue #839). |
| `soroban_pulse_push_delivery_latency_seconds` | Histogram | `device_type` | Push delivery latency. |
| `soroban_pulse_web_push_sent_total` / `web_push_failed_total` | Counter | — | Web Push (browser) delivery outcomes (issue #839). |
| `soroban_pulse_batch_delivered_total` | Counter | — | Total events delivered as part of batches. |
| `soroban_pulse_batch_deliveries_total` | Counter | — | Number of batch delivery operations. |
| `soroban_pulse_batch_delivery_failures_total` | Counter | — | Failed batch deliveries. |
| `soroban_pulse_batch_config_updates_total` | Counter | — | Batch delivery config changes. |
| `soroban_pulse_subscriptions_paused_total` / `_resumed_total` | Counter | — | Manual subscription pause/resume events (issue #884). |
| `soroban_pulse_subscriptions_auto_resumed_total` | Counter | — | Subscriptions auto-resumed after their pause window elapsed. |

## Message queues & event publishing

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_kinesis_publish_failures_total` | Counter | — | AWS Kinesis publish failures. |
| `soroban_pulse_kinesis_throttled_total` | Counter | — | Kinesis `ProvisionedThroughputExceededException`s. |
| `soroban_pulse_pubsub_publish_failures_total` | Counter | — | GCP Pub/Sub publish failures. |
| `soroban_pulse_pubsub_ordering_key_set_total` | Counter | — | Messages published with an ordering key set (issue #398). |
| `soroban_pulse_redis_publish_failures_total` | Counter | — | Redis queue publish failures (all retries exhausted). |
| `soroban_pulse_redis_dropped_total` | Counter | — | Events dropped because the Redis in-memory buffer was full. |
| `soroban_pulse_redis_reconnect_total` | Counter | — | Successful Redis reconnections after a connection loss. |
| `soroban_pulse_redis_buffer_size` | Gauge | — | Current size of the Redis in-memory buffer. |
| `soroban_pulse_replay_jobs_total` | Counter | — | Event replay jobs executed. |

## Security & compliance

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_reencrypt_progress` | Gauge | — | Remaining rows to re-encrypt during a key-rotation run (issue #372). |
| `soroban_pulse_reencrypt_errors_total` | Counter | — | Re-encryption errors. |
| `soroban_pulse_anonymization_applied_total` | Counter | `rule` | Anonymization rule applications (issue #618). |
| `soroban_pulse_pii_detected_total` | Counter | `field` | PII detections by field name. |
| `soroban_pulse_abi_cache_hits_total` / `_misses_total` | Counter | `contract_id` | Contract ABI cache hit/miss (issue #607). |
| `soroban_pulse_abi_validation_failures_total` | Counter | `contract_id` | Contract ABI validation failures. |
| `soroban_pulse_abi_cache_evictions_total` | Counter | — | ABI cache evictions. |
| `soroban_pulse_ledger_hash_mismatches_total` | Counter | `ledger` | Ledger hash chain mismatches detected (issue #608) — should always be `0` in a healthy deployment; any increment is a data-integrity incident. |
| `soroban_pulse_ledger_hashes_verified_total` | Counter | — | Ledger hashes successfully verified. |
| `soroban_pulse_ledger_hash_chain_height` | Gauge | — | Highest ledger verified in the hash chain. |
| `soroban_pulse_lua_timeout_total` | Counter | — | Lua transformation script timeouts. |
| `soroban_pulse_feature_flag_rollback_total` | Counter | `flag_name` | Automatic feature-flag rollbacks (issue #587). |
| `soroban_pulse_feature_flag_error_rate` | Gauge | — | Error rate feeding the auto-rollback decision. |

## Backups

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_backup_verification_success_total` / `_failure_total` | Counter | — | Backup verification job outcomes (issue #894). |
| `soroban_pulse_backup_size_bytes` | Gauge | — | Size of the most recently verified backup. |
| `soroban_pulse_backup_duration_seconds` | Gauge | — | Time taken to produce the backup. |
| `soroban_pulse_restore_duration_seconds` | Gauge | — | Time taken for a test restore. |
| `soroban_pulse_backup_integrity_verified_total` | Counter | — | Integrity checks passed. |
| `soroban_pulse_backup_encryption_verified_total` | Counter | — | Encryption-at-rest checks passed. |
| `soroban_pulse_backup_row_count_verified_total` | Counter | — | Row-count reconciliation checks passed. |

See [backup-verification.md](backup-verification.md) for the verification job itself.

## Distributed tracing & alert lifecycle

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_trace_spans_created_total` | Counter | `span_name` | Trace spans created (issue #895). |
| `soroban_pulse_trace_samples_total` | Counter | `sampled` (`true`\|`false`) | Trace sampling decisions. |
| `soroban_pulse_trace_sample_rate` | Gauge | — | Currently configured trace sample rate `[0, 1]`. |
| `soroban_pulse_trace_injection_latency_ms` | Gauge | — | Latency added by trace-context injection. |
| `soroban_pulse_alerts_fired_total` | Counter | `alert_name`, `severity` | Alerts fired through the issue #897 alert lifecycle API. |
| `soroban_pulse_alerts_resolved_total` | Counter | `alert_name` | Alerts resolved. |
| `soroban_pulse_alerts_silenced_total` | Counter | `alert_name` | Alerts silenced. |
| `soroban_pulse_alert_silence_duration_minutes` | Gauge | `alert_name` | Configured silence duration. |
| `soroban_pulse_active_alerts` | Gauge | `component` | Currently active (unresolved) alerts per component. |
| `soroban_pulse_alerts_total` | Counter | `severity`, `component` | A second, independent alert counter emitted by `src/alert_manager.rs`'s in-process alert manager — distinct from `alerts_fired_total` above, which comes from the issue #897 HTTP-facing alert API. Don't conflate the two when building a "total alerts" panel. |

## SLI / SLO

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_slo_completion_ratio` | Gauge | `slo`, `component` | Fraction of the rolling window that met the SLO target, `[0, 1]`. |
| `soroban_pulse_slo_error_budget_remaining` | Gauge | `slo`, `component` | Fraction of error budget left, `[0, 1]`. `0` = budget exhausted. |
| `soroban_pulse_slo_error_budget_consumed` | Gauge | `slo`, `component` | Complement of the above, `[0, 1]`. |
| `soroban_pulse_slo_burn_rate` | Gauge | `slo`, `component` | `1.0` = on track to exhaust the budget exactly at window end; `>2` = burning faster than sustainable (Google SRE workbook convention). |
| `soroban_pulse_sli_current_value` | Gauge | `slo`, `component` | Most recent raw SLI observation. |
| `soroban_pulse_slo_evaluation_total` | Counter | `slo`, `status` | SLO status transitions (Met → AtRisk → Breached). |
| `soroban_pulse_slo_tracked_count` | Gauge | — | Total SLOs currently tracked. |
| `soroban_pulse_slo_met_count` / `_at_risk_count` / `_breached_count` | Gauge | — | SLO counts by current status. |
| `soroban_pulse_slo_budget_burndown` | Gauge | `slo` | Budget consumed to date (issue #896). |
| `soroban_pulse_sli_latency_percentile` | Gauge | `percentile` | Latest computed latency percentile (issue #896). |
| `soroban_pulse_sli_error_rate` | Gauge | `endpoint` | Per-endpoint error rate SLI. |
| `soroban_pulse_sli_availability` | Gauge | `endpoint` | Per-endpoint availability SLI. |

Full conceptual background: [sli-slo.md](sli-slo.md).

## Anomaly detection

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_anomaly_detection_configured_total` | Counter | — | Anomaly-detection configs created (issue #882). |
| `soroban_pulse_anomaly_alerts_queried_total` | Counter | — | Anomaly alert queries served. |
| `soroban_pulse_anomaly_alert_acknowledged_total` | Counter | — | Anomaly alerts acknowledged. |
| `soroban_pulse_anomaly_score` | Histogram | `metric` | Distribution of computed anomaly scores per monitored metric. |
| `soroban_pulse_anomaly_threshold_crossings_total` | Counter | `metric`, `severity` | Threshold-crossing events. |

## Resource utilization

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_process_memory_bytes` | Gauge | — | Process RSS, read from `/proc/self/status` (Linux only), refreshed every 30s. Used by `PodMemoryNearLimit`. |
| `soroban_pulse_process_memory_rss_bytes` / `_vms_bytes` | Gauge | — | RSS/VMS from the resource-utilization collector (issue #630) — a second, independent memory sampler from `update_process_memory_bytes()` above. |
| `soroban_pulse_fd_count` | Gauge | — | Open file descriptor count. |
| `soroban_pulse_disk_read_bytes_total` / `disk_write_bytes_total` | Gauge | — | Cumulative disk I/O bytes (despite the `_total` suffix these are gauges snapshotting `/proc` counters, not Prometheus counters — don't wrap in `rate()` expecting a monotonic reset-safe counter type, though the underlying value is monotonic until process restart). |
| `soroban_pulse_disk_syscalls_read_total` / `disk_syscalls_write_total` | Gauge | — | Cumulative read/write syscall counts, same caveat as above. |

---

## Interpretation & alerting

Alert thresholds live in [`docs/alerts.yml`](alerts.yml) and load into
Prometheus via `prometheus.yml`'s `rule_files`. This table is the "when to
alert, normal range" cross-reference the checklist asks for — it's derived
directly from that file, not independently invented, so it stays in sync as
long as `alerts.yml` does.

| Metric | Alert | Condition | For | Severity | What it means |
|---|---|---|---|---|---|
| `soroban_pulse_indexer_lag_ledgers` | `IndexerLagHigh` | `> 100` | 5m | warning | Indexer falling behind chain head — check RPC latency and DB write throughput. |
| `soroban_pulse_indexer_lag_ledgers` | `IndexerLagCritical` | `> 500` | 10m | critical | Sustained lag; consumers are seeing stale data. |
| `soroban_pulse_rpc_errors_total` | `IndexerRPCErrors` | `rate(...[5m]) > 0.1` | 5m | warning | RPC endpoint intermittently failing. |
| `soroban_pulse_rpc_errors_total` | `HighRPCErrorRate` | error ratio over 5m | 5m | critical | RPC endpoint largely unavailable — the indexer will stall soon if this doesn't clear. |
| `soroban_pulse_events_indexed_total` | `IndexerNoEventsIndexed` | `increase(...[10m]) == 0` | 15m | warning | No events indexed in 10 minutes — could be a genuinely quiet chain or a stuck indexer; cross-check `indexer_lag_ledgers`. |
| `soroban_pulse_db_pool_size` / `db_pool_max_connections` | `DBPoolExhaustion` | `size >= max` | 1m | critical | Pool fully saturated — see [connection-pool.md](connection-pool.md). |
| `soroban_pulse_http_request_duration_seconds` | `HighHTTPErrorRate` | 5xx ratio over 5m | 5m | critical | API serving elevated error responses. |
| `soroban_pulse_http_request_duration_seconds` | `P99LatencySLOBreach` | `p99 > 0.2s` | 5m | critical | Tightest latency SLO breached. |
| `soroban_pulse_http_request_duration_seconds` | `HTTPRequestLatencyHigh` | `p95 > 1s` | 5m | warning | Earlier warning ahead of the p99 critical alert. |
| `soroban_pulse_process_memory_bytes` | `PodMemoryNearLimit` | `> 90%` of 512Mi | 5m | warning | Adjust the container memory limit assumption in `alerts.yml` if your deployment sizes pods differently. |
| `soroban_pulse_unused_indexes_total` | `UnusedIndexesDetected` | `> 0` | 24h | warning | See the [index-maintenance.md](index-maintenance.md) runbook before dropping anything. |
| `soroban_pulse_matview_refresh_timeout_total` | `MatviewRefreshTimeout` | `increase(...[1h]) > 0` | 0m | warning | A materialized view refresh hit a lock timeout instead of completing. |
| `soroban_pulse_notification_delivery_*` | `NotificationDeliverySLABreach` | failure ratio over window | 2m | critical | See [notification-features.md](notification-features.md). |
| `soroban_pulse_notification_delivery_latency_seconds` | `NotificationDeliveryLatencyHigh` | latency threshold | 5m | warning | Delivery is succeeding but slowly. |
| `soroban_pulse_replica_replay_lag_seconds` | `ReplicaLagHigh` / `ReplicaLagCritical` | `> 30` / `> 120` | 2m / 5m | warning / critical | See [replica-monitoring.md](replica-monitoring.md). |
| `soroban_pulse_replica_lag_bytes` | `ReplicaLagBytes` | `> 100 MiB` | 5m | warning | Byte-lag view of the same condition — catches lag that hasn't yet translated into seconds (e.g. large single transaction). |
| `soroban_pulse_replica_count` | `ReplicaDown` | `== 0` | 5m | critical | No replicas visible at all. |
| `soroban_pulse_slo_burn_rate` | `SLOBudgetBurnRateFast` / `Slow` | `> 14.4` / `> 6` | 2m / 5m | critical / warning | Google SRE multi-window burn-rate alerting — see [sli-slo.md](sli-slo.md). |
| `soroban_pulse_slo_error_budget_remaining` | `SLOErrorBudgetLow` | `< 0.1` | 10m | warning | Less than 10% of the error budget left in the window. |
| `soroban_pulse_slo_completion_ratio` | `SLOCompletionBelowTarget` / `SLOSeverelyBreached` | `< 0.95` / `< 0.5` | 15m / 5m | warning / critical | |
| `soroban_pulse_feature_flag_rollback_total` | `FeatureFlagAutoRollback` | `increase(...[5m]) > 0` | 0m | warning | A flag was auto-rolled-back; check what triggered it. |
| `soroban_pulse_feature_flag_error_rate` | `HighErrorRateRollbackRisk` | `> 0.03` | 2m | warning | Error rate approaching the auto-rollback threshold. |

### Known discrepancies between docs and code

Found while cross-referencing `alerts.yml` and the Grafana dashboards against
the actual metric names in source — worth fixing, listed here so they don't
silently cost you an alert:

- **`IndexerStall` alert is dead.** Its expression reads
  `time() - soroban_pulse_indexer_last_poll_timestamp > 120`, but no code
  path emits a `soroban_pulse_indexer_last_poll_timestamp` gauge — the last
  successful poll is tracked only in-process (`HealthState::update_last_poll`,
  `src/config.rs`) and surfaced via `/healthz/ready`, not `/metrics`. Either
  add the gauge or drive this alert off `indexer_lag_ledgers` /
  `events_indexed_total` instead.
- **`DBPoolExhaustion`'s condition is loose.** It reads `size >= max`, but the
  gauge to compare against is `soroban_pulse_db_pool_max_connections`
  (`alerts.yml` currently references it without the `_connections` suffix in
  one place — verify the loaded rule matches the exported name before relying
  on it). `soroban_pulse_db_pool_utilization >= 0.9` is the more direct
  signal and is what `record_pool_exhaustion_alert()` itself uses internally.
- **`docs/grafana-dashboard.json`'s "Active SSE Connections" panel queries
  `soroban_pulse_sse_connections_active`**, but the metric emitted by
  `update_sse_connections()` is `soroban_pulse_sse_active_connections`
  (confirmed correct in `docs/performance-regression-dashboard.json`'s panel
  of the same name). The panel in the first dashboard will render empty.

## Metric correlation guide

Single metrics rarely tell the whole story. These are the pairings/groupings
worth checking together:

- **Indexer health**: `indexer_lag_ledgers` rising + `rpc_errors_total`
  rising together points at the RPC endpoint; `indexer_lag_ledgers` rising
  with `rpc_errors_total` flat points at DB-side slowness — check
  `db_pool_utilization` and `query_duration_seconds` next.
- **DB pool exhaustion**: `db_pool_utilization` → `db_pool_acquire_latency_seconds`
  → `http_request_duration_seconds` (p99). Utilization saturating shows up as
  acquire latency first, then as end-to-end request latency once callers are
  queued waiting for a connection.
- **Replica health**: `replica_count` and `replica_health_score` are the
  headline signals; `replica_replay_lag_seconds` and `replica_lag_bytes`
  should move together — if bytes lag is high but seconds lag is low, a large
  single transaction is in flight rather than sustained replay slowness.
- **Query planning drift**: `query_plan_cache_hit_ratio` dropping alongside
  `query_planning_time_ms` rising means plans are being evicted and
  re-planned more often than expected — check
  `query_plan_cache_entry_count` against the configured cache size.
- **SSE backpressure**: `sse_ring_buffer_overflows_total` rising with
  `sse_lagged_events_total` rising for the same connections means clients
  aren't draining fast enough; `sse_ring_buffer_misses_total` rising on top
  of that means reconnects are falling back to (slower) DB replay.
- **SLO burn**: `slo_burn_rate` is the leading indicator;
  `slo_error_budget_remaining` is the lagging one. A fast burn rate with
  budget still comfortable is an early warning; a low budget with burn rate
  back to ~1.0 means damage is already done but stabilized.
- **Notification delivery**: a spike in any per-channel `_failures_total`
  (webhook/email/push/Slack/etc.) should be cross-checked against
  `notification_channel_healthy` for that channel before assuming it's a
  transient blip.

## Dashboard examples

Three Grafana dashboards ship in `docs/`:

| Dashboard | Panels | Focus |
|---|---|---|
| [`grafana-dashboard.json`](grafana-dashboard.json) | 22 | General operations: indexer lag/throughput, HTTP latency/error rate, DB pool, index health, event flow, notification delivery, contract popularity. |
| [`sli-slo-dashboard.json`](sli-slo-dashboard.json) | 14 | SLO status, completion ratio, error budget, burn rate, SLI trend, latency percentiles, per-endpoint availability. |
| [`performance-regression-dashboard.json`](performance-regression-dashboard.json) | 23 | Load-test / regression tracking: p50/p95/p99 by route, throughput, error rate, memory soak trend, baseline-ratio panel, load-test run log. |

Import any of them directly into Grafana (`Dashboards → Import → Upload
JSON`), or via the provisioning config in
[`docs/grafana/provisioning/`](grafana/provisioning) for automated setup.
`docs/grafana-dashboard.json` templates on a `$instance` variable — set it to
match your Prometheus `instance` label before the panels populate.

Example panel query, taken directly from `performance-regression-dashboard.json`
(p99 latency by route):

```promql
histogram_quantile(
  0.99,
  sum(rate(soroban_pulse_http_request_duration_seconds_bucket[5m])) by (le, route)
)
```

And the SLO burn-rate panel from `sli-slo-dashboard.json`:

```promql
soroban_pulse_slo_burn_rate
```

## Metric collection and storage

- **Exporter**: `metrics_exporter_prometheus`, installed once at startup via
  `metrics::init_metrics()` (`src/metrics.rs`), which also sets custom
  histogram buckets `[0.05, 0.1, 0.2, 0.5, 1.0, 5.0]`s specifically for
  `soroban_pulse_http_request_duration_seconds` — every other histogram in
  this doc uses the exporter's default bucket set.
- **Scrape target**: `GET /metrics` on the app's HTTP port, wired in
  `prometheus.yml`:

  ```yaml
  scrape_configs:
    - job_name: soroban-pulse
      metrics_path: /metrics
      static_configs:
        - targets: [app:3000]
  scrape_interval: 15s
  ```

- **Rule loading**: `prometheus.yml` loads `/etc/prometheus/alerts.yml`
  (mount `docs/alerts.yml` there, or point `rule_files` at the repo path in
  local/dev setups).
- **Push vs. pull**: all metrics here are pull-based (standard Prometheus
  scrape). Nothing in this codebase pushes to a Pushgateway.
- **Kubernetes**: see [kubernetes-probes.md](kubernetes-probes.md) for how
  liveness/readiness probes relate to (but are distinct from) the metrics
  endpoint — probes don't go through Prometheus at all.

## Historical retention

- **Prometheus retention** is controlled by Prometheus itself (`--storage.tsdb.retention.time`),
  not by anything in this repo — `prometheus.yml` here doesn't set it, so a
  default Prometheus install retains 15 days. For SLO tracking over a rolling
  30-day window (see [sli-slo.md](sli-slo.md)), set retention to at least
  30–45 days, or federate/remote-write into longer-term storage (Thanos,
  Cortex, Mimir, or a managed Prometheus-compatible backend) if you need
  longer trend analysis than local TSDB retention supports.
- **High-cardinality metrics** — `contract_event_count` (per `contract_id`),
  `sse_lagged_events_total` (per `connection_id`), and the per-index metrics
  in `index_scan_count`/`index_bloat_ratio`/`index_size_bytes`/`index_dead_tuples`
  (per `table`+`index`) — are the ones most likely to blow up TSDB size over a
  long retention window. If retention cost becomes an issue, these are the
  first candidates for a shorter retention class via Prometheus recording
  rules + downsampling, rather than shortening retention globally.
- **Event data retention** (the underlying indexed event rows, as opposed to
  the metrics describing them) is a separate policy — see
  [data-retention.md](data-retention.md).

## Metric search tool

For a quick local lookup instead of scrolling this file,
[`scripts/metrics-search.sh`](../scripts/metrics-search.sh) greps both this
reference and the source definitions:

```bash
./scripts/metrics-search.sh lag
# soroban_pulse_indexer_lag_ledgers        src/metrics.rs:42   Update the indexer lag
# soroban_pulse_replica_replay_lag_seconds src/replica_monitor.rs
# ...
```

It takes one substring argument and prints every matching metric name found
under `src/`, alongside the `docs/metrics-reference.md` table row describing
it when one exists — useful when you only remember part of a name while
writing a PromQL query or a new alert rule.

## Related documentation

- [alerts.yml](alerts.yml) — the Prometheus alert rules referenced throughout this doc
- [alerting.md](alerting.md) — Alertmanager routing/notification setup
- [sli-slo.md](sli-slo.md) — SLI/SLO concepts and the tracker behind the `slo_*` metrics
- [connection-pool.md](connection-pool.md) — DB pool sizing and exhaustion behavior
- [replica-monitoring.md](replica-monitoring.md) — replication lag monitoring
- [chaos-testing.md](chaos-testing.md) — how failure scenarios are expected to move these metrics
- [performance-tuning.md](performance-tuning.md) — using these metrics to size and tune the service
