# Multi-Region Deployment (Issue #909)

Infrastructure and operational guidance for running SorobanPulse across multiple AWS regions for latency reduction and geo-redundancy.

This document covers the **infrastructure layer** (Terraform, Global Accelerator, cross-region metrics). For the **application-layer** replication model — single-writer indexing, advisory locks, PostgreSQL streaming replication, and manual/automated failover procedures — see [multi-deployment-architecture.md](multi-deployment-architecture.md), which this design builds on.

## Overview

```
                    ┌─────────────────────────┐
                    │   AWS Global Accelerator │
                    │   (anycast static IPs)   │
                    └────────────┬────────────┘
              ┌──────────────────┼──────────────────┐
        100%  │             80%  │             80%   │  (traffic dial)
              ▼                  ▼                   ▼
   ┌────────────────┐  ┌────────────────┐  ┌────────────────────┐
   │  us-east-1      │  │  eu-west-1      │  │  ap-southeast-1     │
   │  (primary)      │  │  (secondary)    │  │  (tertiary)         │
   │  ASG 3-10       │  │  ASG 2-8        │  │  ASG 2-8            │
   │  RDS r6g.2xl     │  │  RDS r6g.xl      │  │  RDS r6g.xl          │
   │  2 read replicas│  │  1 read replica │  │  1 read replica     │
   └────────────────┘  └────────────────┘  └────────────────────┘
```

## Terraform Layout

Multi-region infrastructure is defined in [`terraform/multi-region.tf`](../terraform/multi-region.tf), separate from the single-region root module described in [terraform.md](terraform.md).

| Region | Role | Instance type | ASG size | DB instance | Read replicas |
|---|---|---|---|---|---|
| `us-east-1` | Primary | `t3.xlarge` | 3–10 | `db.r6g.2xlarge`, Multi-AZ | 2 |
| `eu-west-1` | Secondary | `t3.large` | 2–8 | `db.r6g.xlarge`, Multi-AZ | 1 |
| `ap-southeast-1` | Tertiary | `t3.large` | 2–8 | `db.r6g.xlarge`, Multi-AZ | 1 |

Each region is instantiated via `module "soroban_pulse_<region>" { source = "./modules/soroban-pulse" ... }`.

> **Known gap:** `terraform/multi-region.tf` references a `modules/soroban-pulse` module that does not yet exist under `terraform/modules/` (only `vpc`, `rds`, `alb`, `ecs`, `monitoring`, and `backup` are implemented today). Building that composite module — wiring the existing `vpc`/`rds`/`alb`/`ecs` modules together per-region — is a prerequisite for `terraform plan` to succeed against this file. Track this before running `terraform apply` on multi-region infrastructure.

## Load Balancing and Failover Routing

Cross-region routing uses **AWS Global Accelerator** rather than DNS-based routing, so clients get static anycast IPs and fast (TCP-level) failover without DNS TTL delays.

- `aws_globalaccelerator_accelerator` — one accelerator per environment, flow logs enabled to S3.
- `aws_globalaccelerator_listener` — TCP listener on port 443.
- `aws_globalaccelerator_endpoint_group` per region, pointing at that region's ALB:
  - Health check: `GET /health` every 30s, HTTPS, unhealthy after 3 failed checks.
  - Traffic dial: 100% (us-east-1), 80% (eu-west-1, ap-southeast-1) — controls the fraction of that region's endpoint capacity Global Accelerator will use even when healthy.
  - Endpoint weight: 128 (us-east-1) vs 64 (eu-west-1, ap-southeast-1) — controls relative share of traffic routed to each healthy endpoint within a region via latency-based routing.

When a region's health check fails, Global Accelerator automatically stops routing new connections to that endpoint group and shifts traffic to the remaining healthy regions — no manual DNS cutover required for read/HTTP traffic. Write-path routing to the single active primary still follows the advisory-lock model in [multi-deployment-architecture.md](multi-deployment-architecture.md#data-consistency).

Outputs: `global_accelerator_dns`, `global_accelerator_ips` (static anycast IPs — use these for firewall allowlisting instead of per-region ALB IPs, which can change).

## Data Replication

See [multi-deployment-architecture.md § Cross-Region Sync](multi-deployment-architecture.md#cross-region-sync) for the full replication model. Summary:

- PostgreSQL physical (streaming) replication from the `us-east-1` primary to read replicas in `eu-west-1` and `ap-southeast-1`.
- Secrets (`API_KEY`, `WEBHOOK_SECRET`, `EVENT_DATA_ENCRYPTION_KEY`) are synced via AWS Secrets Manager and **must be identical across regions** — a mismatched `EVENT_DATA_ENCRYPTION_KEY` will make encrypted event payloads unreadable after failover.
- Subscriptions and webhook registrations live in PostgreSQL and replicate with the rest of the database — no separate sync mechanism.

## Conflict Resolution for Writes

SorobanPulse uses a **single-writer model**, not multi-master — this sidesteps write-write conflict resolution entirely:

- Exactly one region holds the indexer advisory lock at a time and is the only writer for indexed event data.
- All admin operations, subscription/webhook registration, and replay/backfill jobs are routed to the primary region (see the routing table in [multi-deployment-architecture.md § Data Consistency](multi-deployment-architecture.md#data-consistency)).
- On failover, the newly promoted region acquires the advisory lock and resumes writes from the last `indexer_checkpoints` row — idempotent consumers absorb any duplicate reads that occurred during promotion.
- If a future active-active write path is introduced, it must define per-table conflict resolution (e.g., last-write-wins on `updated_at`, or CRDT-style merge for counters) before enabling concurrent writers in more than one region.

## Cross-Region Latency Metrics

Recommended metrics to add to `docs/alerts.yml` / Grafana alongside the existing replication-lag alert:

| Metric | Source | Purpose |
|---|---|---|
| `soroban_pulse_indexer_lag` | App | Existing — indexer distance from chain tip (per region) |
| `pg_replication_lag_seconds` | Postgres exporter | Existing — used in the `ReplicationLagHigh` alert |
| Global Accelerator flow logs (S3) | AWS | Per-connection latency and endpoint selection, queryable via Athena |
| `soroban_pulse_cross_region_rtt_seconds` (proposed) | Synthetic probe hitting `/healthz/ready` in each region from each region | Measures inter-region network latency directly, independent of client location |
| ALB `TargetResponseTime` p50/p99 per region | CloudWatch (`modules/monitoring`) | Per-region API latency for comparison |

Cross-region latency between `us-east-1` ↔ `eu-west-1` typically runs ~70–90ms, and `us-east-1` ↔ `ap-southeast-1` ~170–200ms; budget these into any synchronous cross-region health checks or admin tooling.

## Compliance and Data Residency

- Global Accelerator flow logs and RDS storage are region-local; no event data is copied outside a region except via the explicit replication streams above.
- For deployments with data-residency requirements (e.g., GDPR, in-region-only processing for EU users), disable cross-region read replicas for the affected region and serve reads exclusively from the local database — this trades reduced read availability during a regional outage for residency guarantees.
- Secrets replicated via AWS Secrets Manager should use per-region KMS keys, not a shared cross-region key, when residency policy prohibits key material leaving a region.
- Document any region-specific data-handling exceptions in `terraform.tfvars` comments for that region's environment file so the exception is visible at `terraform plan` time.

## Testing

- `terraform validate` / `terraform plan` against `terraform/multi-region.tf` (blocked until `modules/soroban-pulse` exists — see gap above).
- Global Accelerator health-check behavior: simulate a regional outage by scaling an ASG to 0 and confirming the endpoint group is marked unhealthy and traffic shifts within ~90s (3 × 30s health-check interval).
- Failover drills should follow the manual failover procedure in [multi-deployment-architecture.md § Manual Failover Procedure](multi-deployment-architecture.md#manual-failover-procedure) and the DR game-day process in [disaster-recovery.md](disaster-recovery.md).

## Related Documentation

- [multi-deployment-architecture.md](multi-deployment-architecture.md) — replication patterns, failover procedures, RTO table
- [terraform.md](terraform.md) — single-region Terraform module reference
- [disaster-recovery.md](disaster-recovery.md) — RTO/RPO targets and DR testing
- [iac-testing.md](iac-testing.md) — Terraform validation and testing pipeline
