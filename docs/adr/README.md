# Architecture Decision Records

Architecture Decision Records (ADRs) capture decisions that affect SorobanPulse’s structure, interfaces, operations, or long-term maintenance. An ADR records the context and trade-offs at the time of a decision; it is not a task specification or an implementation checklist.

## Index

| ADR | Decision | Status |
|---|---|---|
| [0001 — ADR system](0001-adr-system.md) | Use numbered Markdown ADRs as the canonical decision log | Accepted |

New decisions must be added to this table and linked from the relevant code or documentation. Use the next available four-digit number; never renumber an existing ADR.

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

## Naming

Use `NNNN-short-title.md`, where `NNNN` is a monotonically increasing number. Keep filenames stable because external references may link directly to them.
