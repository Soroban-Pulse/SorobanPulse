# Feature Maturity

This page records the maturity of each subsystem in the repository so that readers, operators and auditors can tell which features are production-ready and which are stubs, incomplete, or not yet implemented.

> **Why this page exists:** several docs previously described features as complete when the code shows they are stubs or not wired up. Relying on those claims can lead to insecure deployments (for example, believing bounce webhooks are authenticated). See tracking issue [#1484](https://github.com/OWNER/REPO/issues/1484).

## Legend

| Level | Meaning |
| --- | --- |
| **Stable** | Implemented, tested, and safe to rely on in production. |
| **Beta** | Implemented and usable, but the API or behaviour may change and coverage is incomplete. |
| **Experimental** | Partially implemented or a stub. Behaviour may be missing, incorrect, or insecure. Do not rely on it in production. |
| **Planned** | Not implemented. Configuration or feature flags may exist, but there is no working code. |

## Subsystem status

| Subsystem | Maturity | Notes | Tracking issue |
| --- | --- | --- | --- |
| SaaS platform | **Experimental** | Reports zero storage and bandwidth; metering is not wired up. | [#1484](https://github.com/OWNER/REPO/issues/1484) |
| ML integration | **Experimental** | Integration is incomplete; do not depend on it for production workloads. | [#1484](https://github.com/OWNER/REPO/issues/1484) |
| GraphQL API | **Experimental** | Routes return an empty `Router::new()` at `src/routes.rs:1016`; no queries or mutations are served. | [#1484](https://github.com/OWNER/REPO/issues/1484) |
| Post-quantum crypto (`quantum-crypto` feature) | **Planned** | The `quantum-crypto` feature is declared in `Cargo.toml` but has no usage in the codebase. | [#1484](https://github.com/OWNER/REPO/issues/1484) |
| Email bounce signature verification | **Experimental** | Signature verification is a stub; bounce webhooks are **not** authenticated. | [#1484](https://github.com/OWNER/REPO/issues/1484) |

## Keeping this table current

Updating this table is part of the PR template for feature work. Any pull request that adds, changes, or removes a subsystem — or that changes how complete a subsystem is — must update the corresponding row here (maturity level, notes, and tracking issue) in the same PR.

Affected pages carry a status banner at the top that must match the level recorded here:

- [`SAAS_PLATFORM.md`](SAAS_PLATFORM.md)
- [`ML_INTEGRATION.md`](ML_INTEGRATION.md)
- GraphQL documentation
- Post-quantum crypto documentation
- Email bounce / webhook documentation