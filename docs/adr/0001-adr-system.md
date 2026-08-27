# 0001 — Use a numbered Markdown ADR system

- **Status:** Accepted
- **Date:** 2026-08-27
- **Owners:** SorobanPulse maintainers
- **Related:** [ADR system guide](README.md)

## Context

SorobanPulse has important architectural decisions distributed across general documentation, issue discussions, and implementation details. Without a consistent record, contributors cannot reliably discover why a public interface, deployment topology, storage behavior, or security boundary was chosen. Future changes may therefore repeat rejected approaches or unintentionally break an operational assumption.

## Decision

SorobanPulse will maintain architecture decisions as numbered Markdown files under `docs/adr/`. Every record uses the shared template, appears in the ADR index, and follows the lifecycle `Proposed`, `Accepted`, `Superseded`, or `Deprecated`. Historical records are retained and replacements link back to the decision they supersede.

ADRs are required for decisions that affect public APIs or events, storage or migrations, security boundaries, deployment topology, data retention, or significant operational dependencies. Routine fixes and localized refactors do not require an ADR unless they introduce one of these impacts.

## Alternatives considered

### Continue using unstructured documentation

This requires less initial process, but makes decisions difficult to find and mixes rationale with current operational instructions. It was rejected because historical context is not clearly separated from normative guidance.

### Store decisions only in issues and pull requests

Issues and pull requests provide useful discussion but are mutable, distributed, and difficult to discover after closure. They were rejected as the canonical record, although ADRs may link to them as supporting references.

### Use an external documentation platform

An external platform could provide richer navigation, but would add an access dependency and make versioning alongside code more difficult. It was rejected in favor of Markdown stored and reviewed with the repository.

## Consequences

The repository gains a stable, searchable decision history that can be reviewed with code changes. Contributors must keep the index and status fields current, and maintainers must preserve old records rather than editing history silently. The lightweight Markdown format works offline and renders in GitHub without additional tooling.

## Rollout and migration

The ADR directory, template, index, and this foundational record establish the system. Existing architectural guidance remains authoritative until a future ADR explicitly supersedes it; contributors should add links from new ADRs to the relevant current documentation and implementation.

## References

- [ADR authoring guide](README.md)
- [ADR template](0000-template.md)
