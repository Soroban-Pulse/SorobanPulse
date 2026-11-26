# CI Workflows

This document is the authoritative overview of every GitHub Actions workflow in
`.github/workflows/`. Each responsibility is covered by exactly one workflow so
that CI minutes are not wasted, releases and publishes run once per tag, and it
is always clear which workflow is authoritative.

## Shared setup

Common setup steps are extracted into composite actions under `.github/actions/`
so workflows stay small and consistent:

| Action | Purpose |
|---|---|
| `.github/actions/setup-rust` | Installs the Rust toolchain, restores the cargo cache, and configures `sccache`. |
| `.github/actions/setup-postgres` | Starts a Postgres service container and waits for it to accept connections. |

## Workflows

### `ci.yml` — CI

- **Purpose:** Primary continuous integration. Runs formatting, linting, tests,
  the `deny` security check, and the `helm-lint` job.
- **Triggers:** `push` to `main`, `pull_request`.
- **Required secrets:** None.

### `docs.yml` — Docs

- **Purpose:** Builds the documentation site and deploys it. This is the single
  authoritative docs workflow (the former `deploy-docs.yml` was merged into it).
- **Triggers:** `push` to `main` (paths under `docs/`), `workflow_dispatch`.
- **Required secrets:** `GITHUB_TOKEN` (default).

### `release.yml` — Release

- **Purpose:** Cuts releases and publishes artifacts. This is the single
  authoritative release workflow (the former `release-please.yml` was merged
  into it). Runs once per tag.
- **Triggers:** `push` of tags matching `v*`, `workflow_dispatch`.
- **Required secrets:** `GITHUB_TOKEN` (default), `CARGO_REGISTRY_TOKEN`.

### `security.yml` — Security

- **Purpose:** Security audit of dependencies. This is the single authoritative
  security audit workflow (the former `cargo-audit.yml` was merged into it; the
  `deny` job in `ci.yml` complements it on pull requests).
- **Triggers:** `push` to `main`, `pull_request`, `schedule` (daily).
- **Required secrets:** None.

### `helm.yml` — Helm

- **Purpose:** Packages and releases Helm charts. This is the single
  authoritative Helm workflow (the former `helm-release.yml` was merged into
  it; the `helm-lint` job in `ci.yml` validates charts on pull requests).
- **Triggers:** `push` to `main` (paths under `charts/`), `push` of tags
  matching `v*`, `workflow_dispatch`.
- **Required secrets:** `GITHUB_TOKEN` (default).

### `vscode-extension-release.yml` — VS Code Extension Release

- **Purpose:** Builds and publishes the VS Code extension. This is the single
  authoritative VS Code publishing workflow (the former `vscode-publish.yml`
  was merged into it). Runs once per tag.
- **Triggers:** `push` of tags matching `v*`, `workflow_dispatch`.
- **Required secrets:** `VSCE_PAT`.

## Conventions

- Prefer reusable composite actions from `.github/actions/` over duplicating
  setup steps.
- Release and publish workflows must be idempotent and guarded so they run at
  most once per tag.
- When adding a new workflow, update this document and confirm it does not
  duplicate an existing responsibility.