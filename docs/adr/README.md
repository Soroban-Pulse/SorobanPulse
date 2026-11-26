# Architecture Decision Records

Architecture Decision Records (ADRs) capture decisions that affect SorobanPulse’s structure, interfaces, operations, or long-term maintenance. An ADR records the context and trade-offs at the time of a decision; it is not a task specification or an implementation checklist.

## Index

| ADR | Decision | Status |
|---|---|---|
| [0001 — ADR system](0001-adr-system.md) | Use numbered Markdown ADRs as the canonical decision log | Accepted |
| [0002 — Multi-replica indexing](0002-multi-replica-indexing.md) | Single-writer indexing with monitored, optionally cross-region read replicas | Accepted |
| [0003 — Webhook retry strategy](0003-webhook-retry-strategy.md) | Layered retry policy, per-endpoint backoff, and circuit breaking for webhook delivery | Accepted |
| [0004 — Event compression](0004-event-compression.md) | Optional application-level gzip compression for stored event data | Accepted |
| [0005 — Aggregation merge](0005-aggregation-merge.md) | Merge aggregation into the indexing pipeline (converted from `aggregation-merge-plan.md`) | Proposed |
| [0006 — Cargo workspace](0006-cargo-workspace.md) | Single Cargo workspace with shared dependency and lint configuration (converted from `cargo-workspace-plan.md`) | Proposed |
| [0007 — Connection pool consolidation](0007-connection-pool-consolidation.md) | Consolidate database connection pooling behind a single shared pool (converted from `connection-pool-consolidation.md`) | Proposed |
| [0008 — Web framework](0008-web-framework.md) | Standardize on axum for HTTP services; actix-web is legacy and not to be extended | Accepted |
| [0009 — Canonical frontend](0009-canonical-frontend.md) | Designate a single canonical frontend and classify the other two as legacy or experimental | Accepted |
| [0010 — SDK packaging](0010-sdk-packaging.md) | Consolidate the two JavaScript SDKs into one published package | Accepted |
| [0011 — Tenancy model](0011-tenancy-model.md) | Adopt a single canonical tenancy model; the alternate model is deprecated | Accepted |
| [0012 — Scripting sandbox](0012-scripting-sandbox.md) | Use sandboxed Lua for user-defined transforms | Accepted |
| [0013 — Event storage and partitioning](0013-event-storage-partitioning.md) | Partition event storage by ledger range with retention-aligned pruning | Accepted |
| [0014 — Telemetry stack](0014-telemetry-stack.md) | OpenTelemetry-based metrics, tracing, and logging pipeline | Accepted |
| [0015 — Feature-flag strategy](0015-feature-flag-strategy.md) | Compile-time Cargo features for build variants; runtime flags for rollout only | Accepted |

New decisions must be added to this table and linked from the relevant code or documentation. Use the next available four-digit number; never renumber an existing ADR.

## Converted planning documents

The following planning documents were converted into ADRs and removed from `docs/` so that each decision carries an explicit status:

| Former document | ADR | Status |
|---|---|---|
| `docs/aggregation-merge-plan.md` | [0005 — Aggregation merge](0005-aggregation-merge.md) | Proposed |
| `docs/cargo-workspace-plan.md` | [0006 — Cargo workspace](0006-cargo-workspace.md) | Proposed |
| `docs/connection-pool-consolidation.md` | [0007 — Connection pool consolidation](0007-connection-pool-consolidation.md) | Proposed |

Do not add new `*-plan.md` documents under `docs/`; record the decision as an ADR instead.

Regenerate a status overview of all ADRs (title, status, date) at any time with `scripts/adr-summary.sh`.

## When to write an ADR

Write an ADR when a change affects a public API, event or storage contract, deployment topology, data-retention behavior, security boundary, operational dependency, or a decision that future contributors may reasonably revisit. Routine bug fixes, dependency patches, and localized refactors do not normally require one.

## Lifecycle

An ADR begins as **Proposed**, becomes **Accepted** after the maintainers approve the decision, and may later become **Superseded** or **Deprecated**. Do not delete historical ADRs. Link a replacement ADR from the superseded record and update the index.

## Authoring workflow

1. Copy [`0000-template.md`](0000-template.md) to the next available number and use a kebab-case title.
2. Describe the problem, constraints, decision, alternatives, consequences, and rollout or migration plan.
3. Link relevant issues, pull requests, runbooks, API documentation, and security considerations.
4. Add the new record to this index and request review in the pull request.
5. Update the status only through a reviewed pull request.

## Review checklist

- The decision is specific enough to guide implementation.
- Alternatives and rejected trade-offs are documented.
- Security, reliability, cost, and operational consequences are explicit.
- Public ABI, storage, migration, and rollback impacts are identified.
- The record links to the current implementation and runbooks.

See [review-guidelines.md](review-guidelines.md) for the full reviewer guide, including examples of vague versus specific decisions, how to handle reviewer disagreement, and when to request splitting an ADR.

## Naming

Use `NNNN-short-title.md`, where `NNNN` is a monotonically increasing number. Keep filenames stable because external references may link directly to them.
