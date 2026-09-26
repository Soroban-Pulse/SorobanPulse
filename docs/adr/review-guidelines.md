# ADR review guidelines

This expands the review checklist in [README.md](README.md) into a fuller guide for reviewing an ADR pull request. It applies to a new ADR, a status change (e.g., `Proposed` → `Accepted`), and a superseding ADR.

## What a reviewer is approving

Approving an ADR is not approving that the decision is optimal; it is approving that the record accurately and specifically describes a decision the team is prepared to stand behind and be held to. A reviewer should read an ADR the way they read a migration: once merged and `Accepted`, it is a commitment other contributors will build on and cite, and changing it later requires a new or superseding ADR, not a silent edit.

## What to push back on

- **Vague scope.** If a reader cannot tell from the Decision section what is and is not covered, send it back. "We will improve webhook reliability" is not a decision; "webhook delivery retries with exponential backoff, jitter, and a per-endpoint circuit breaker, per `RetryPolicy::webhook_default()`" is.
- **Missing or thin alternatives.** An ADR with no rejected alternatives, or alternatives that are strawmen no one seriously considered, is a sign the trade-off was not actually weighed. Ask for the alternatives that were realistically on the table, including "do nothing" or "keep the current behavior," and why each was rejected.
- **Unstated consequences.** Every ADR has a cost — performance, complexity, an on-call burden, a new failure mode, backward-compatibility risk. If the Consequences section only lists benefits, ask the author what breaks or gets harder.
- **No rollback or migration story for anything with state.** If the decision touches storage, a public API, or a deployment topology, "Rollout and migration" must say how to undo it or explicitly say why it cannot be undone. "Not applicable" is fine only when nothing changes on disk, over the wire, or in a running deployment.
- **Claims not grounded in the codebase.** An ADR documenting an existing decision (as opposed to proposing a new one) should cite the actual implementation — file paths, function names, config keys — not a general description of what the code is assumed to do. If the cited behavior doesn't match `main`, the ADR is wrong, not the code.
- **Index and cross-links out of sync.** The ADR must appear in the README index table with the correct status, and code implementing the decision should carry a one-line comment pointing back to it. A merged ADR nobody can find from the code is close to not existing.

## Vague versus specific: examples

**Too vague:**
> We will make the system more resilient to replica failures by adding better monitoring and retry logic.

This says nothing a future contributor could act on or verify. It doesn't say which component, what "better" means, or what "resilient" is measured against.

**Specific enough to approve:**
> Replica lag is polled every 60 seconds from `pg_stat_replication` and exposed as Prometheus gauges (`soroban_pulse_replica_lag_bytes`, `..._replay_lag_seconds`). A warning is logged above 10 MiB / 30 s lag; 100 MiB / 60 s is treated as critical. Read traffic that requires current data (webhook delivery decisions, replay jobs) is routed to the primary, not a replica.

The second version can be checked against the code, gives concrete thresholds, and tells an implementer exactly what to build or verify.

## Handling disagreement

- Disagreement about the decision itself (not the quality of the record) belongs in the pull request discussion, not in silent edits to the ADR text. Let the author update the ADR in response to review comments; don't push commits that change the substance of someone else's decision without discussion.
- If reviewers cannot reach agreement, do not merge a `Proposed` ADR to paper over the disagreement — either resolve it, narrow the ADR's scope to the part that has consensus, or escalate to whoever owns the affected area for a tie-break.
- If an ADR is `Accepted` and a later reviewer disagrees with the original decision, the answer is a new ADR that supersedes it (see the lifecycle in [README.md](README.md)), not a revision of the historical record. The exception is fixing factual errors (a wrong file path, a stale status) — content edits that don't change the decision are fine as normal review feedback.
- Prefer capturing an unresolved concern in the ADR's Consequences section (as a known risk or open question) over blocking merge indefinitely, provided the core decision is sound enough to act on.

## When to request splitting into multiple ADRs

Ask the author to split an ADR when:

- It bundles two decisions that could be accepted, rejected, or superseded independently (e.g., "use gzip for stored events" and "add a circuit breaker for webhook delivery" are unrelated decisions and belong in separate records even if they were implemented in the same pull request).
- The Alternatives or Consequences sections keep drifting between unrelated concerns (storage format in one paragraph, deployment topology in the next) — that's usually a sign of two decisions wearing one title.
- A future contributor would plausibly want to supersede one part without touching the other. If superseding half the ADR would leave the rest still valid, it should have been two ADRs.

Do not split an ADR merely because it is long; a single decision with substantial context, several rejected alternatives, and a detailed rollout plan is normal and should stay together.
