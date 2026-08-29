# 0002 — Single-writer indexing with monitored read replicas

- **Status:** Accepted
- **Date:** 2026-08-29
- **Owners:** SorobanPulse maintainers
- **Related:** [Replica sync monitoring](../replica-monitoring.md), [Event deduplication across replicas](../event_deduplication_replicas.md), [Multi-deployment architecture guide](../multi-deployment-architecture.md)

## Context

SorobanPulse must remain available and keep serving reads during a database or regional outage, and must scale read traffic (dashboards, event search, GraphQL queries) beyond what a single PostgreSQL instance can serve. At the same time, ledger event indexing must not double-process or lose events: two indexer instances writing concurrently would race on ledger cursors and could insert duplicate or conflicting rows.

PostgreSQL streaming replication gives read replicas that lag the primary by a variable, unbounded amount, so any design that fans out indexing or reads to replicas has to say explicitly how staleness and split-brain are bounded. Some deployments also want geo-redundancy across regions or cloud providers, where replication latency and egress cost are materially higher than same-region replication.

## Decision

SorobanPulse uses a single writable primary for event indexing per network, selected via a PostgreSQL advisory lock, with any number of read-only streaming replicas serving HTTP/GraphQL/SSE read traffic (`src/replica_monitor.rs`, `docs/multi-deployment-architecture.md`). Only the instance holding the advisory lock indexes; standby instances serve reads from their local (replica) connection and attempt to acquire the lock on startup or after a connection failure, so promotion is driven by whichever instance reconnects to the new primary first.

Replica health is tracked by a background task that polls `pg_stat_replication` every 60 seconds and exposes per-replica byte lag and write/flush/replay lag as Prometheus gauges, plus an admin endpoint (`GET /v1/admin/replication/status`). Warnings are logged at 10 MiB / 30 s lag and treated as critical at 100 MiB / 60 s (`LAG_WARN_BYTES`, `LAG_WARN_SECS`, `LAG_CRITICAL_BYTES`, `LAG_CRITICAL_SECS` in `src/replica_monitor.rs`), so operators can detect a replica falling behind before it is promoted or used for reads that require freshness.

The replication topology is not limited to plain standby replicas: `ReplicationMode` in `src/replica_monitor.rs` also models cascading replication (a replica streaming to another replica, for large fan-out), dedicated read replicas for query offloading, and experimental bidirectional or selective (table-scoped) logical replication for specialized deployments. Cross-cloud or cross-region replication of published events (as opposed to database replication) is handled separately by `src/cloud_replication.rs`, which lets an operator choose a per-write consistency mode — `Strong` (all providers must ack), `Eventual` (primary must ack, secondaries are best-effort and logged on failure), or `BestEffort` (fire-and-forget, always returns success) — rather than forcing one guarantee on every deployment.

Because more than one replica can independently observe and process the same underlying event stream (for example during failover, or during a cascading topology), event deduplication is coordinated across replicas rather than left to each replica's local state. `src/event_dedup_replicas.rs` adds a distributed layer on top of the existing in-memory bloom filter and database fingerprint check: a PostgreSQL advisory lock derived from the event fingerprint serializes concurrent checks, and a shared `event_dedup_replicas` table records which replica has seen which fingerprint within a rolling window (default one hour), including a `sync_failover_state` path that copies a failed replica's recent fingerprints to the replica taking over.

## Alternatives considered

### Multi-primary / write-anywhere indexing

Allowing every instance to index independently would remove the single point of write coordination but requires conflict resolution for ledger cursors and event inserts, and would make "which instance is authoritative for this ledger range" ambiguous during network partitions. Rejected in favor of advisory-lock-based leader election, which reuses PostgreSQL's existing session semantics and requires no additional consensus system.

### No read replicas (single instance)

Simplest option, but ties read availability to the same instance doing indexing and provides no protection against a regional or provider outage, and no way to scale read throughput independently of indexing throughput. Rejected because SorobanPulse's HTTP/GraphQL/SSE read paths and its indexing path have different scaling and availability needs.

### External replication proxy (e.g., pgpool/pgbouncer-based load balancing)

A connection-pooling proxy could transparently route reads to replicas, but it adds an operational dependency and hides replica lag from the application, making it harder to reject or reroute a request when a specific replica has fallen behind its SLA. Rejected in favor of application-level replica awareness (the admin endpoint and per-workload routing guidance in `docs/multi-deployment-architecture.md`), while still recommending tools like Patroni or pg_auto_failover for automated promotion, which are complementary rather than a substitute.

### Uniform consistency mode for all cross-region replication

Forcing every deployment to use strong consistency would add latency and reduce availability for use cases that can tolerate eventual consistency (e.g., dashboard analytics), while forcing eventual consistency everywhere would be unacceptable for webhook delivery decisions that must not be duplicated. Rejected in favor of the configurable `ConsistencyMode` in `src/cloud_replication.rs`, chosen per deployment.

## Consequences

Read traffic can scale horizontally and survive a primary outage without also having to solve concurrent-write conflicts, and indexing progress is protected by having exactly one writer at a time. Operators get direct visibility into replication health (metrics, admin endpoint, alert thresholds) instead of discovering lag only when reads or failover fail.

The cost is added operational surface: replicas can serve stale reads, so callers that need current data (webhook delivery decisions, replay/backfill jobs, admin operations) must be routed to the primary rather than a replica, per the workload table in `docs/multi-deployment-architecture.md`. Cross-replica deduplication adds a database round trip (advisory lock plus table check, ~5 ms) to the hot path and a table (`event_dedup_replicas`) that must be periodically cleaned up. During simultaneous failure of all replicas, exactly-once processing is not guaranteed — duplicates are an accepted worst case rather than prevented outright.

## Rollout and migration

The replica monitor starts automatically alongside the index monitor; no configuration is required to enable basic lag monitoring. Enabling cross-replica dedup requires the `event_dedup_replicas` table (see `docs/event_deduplication_replicas.md` for schema) and is controlled by `ReplicaDedupConfig::enable_cross_replica_sync`. Multi-region or multi-cloud topologies are opt-in and configured per the patterns in `docs/multi-deployment-architecture.md`; rollback is to fall back to a single-region, single-writer deployment with no read replicas, which requires no data migration since replicas are read-only copies.

## References

- [`src/cloud_replication.rs`](../../src/cloud_replication.rs)
- [`src/replica_monitor.rs`](../../src/replica_monitor.rs)
- [`src/event_dedup_replicas.rs`](../../src/event_dedup_replicas.rs)
- [Replica sync monitoring](../replica-monitoring.md)
- [Event deduplication across replicas](../event_deduplication_replicas.md)
- [Multi-deployment architecture guide](../multi-deployment-architecture.md)
