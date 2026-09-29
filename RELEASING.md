# RELEASING

This document describes the automated release process for SorobanPulse and explains what to do in the rare cases where manual intervention is needed.

---

## Overview

SorobanPulse uses **[release-please]** to manage versioning and changelogs, driven by **[Conventional Commits]**.  The overall flow is:

```
Conventional-commit PRs → merge to main
  → release-please opens a "Release PR"
    → maintainer merges the Release PR
      → release-please creates a GitHub Release + semver tag (v1.2.3)
        → `.github/workflows/release.yml` fires:
            • multi-arch container image pushed to GHCR
            • Linux/macOS binaries built and attested
            • CycloneDX + SPDX SBOMs generated and attested
            • GitHub Release populated with all artefacts
```

No manual version bumping or changelog editing is ever required.

---

## Commit message conventions

Every commit merged to `main` **must** follow [Conventional Commits]:

| Prefix | Changelog section | Version bump |
|--------|-------------------|--------------|
| `feat:` | Features | minor (patch when pre-1.0) |
| `fix:` | Bug Fixes | patch |
| `perf:` | Performance | patch |
| `security:` | Security | patch |
| `refactor:` | Code Refactoring | patch |
| `docs:` | Documentation | patch |
| `deps:` | Dependencies | patch |
| `ci:` | CI/CD | none (hidden) |
| `chore:` | Chores | none (hidden) |
| `test:` | Tests | none (hidden) |

**Breaking changes** — append `!` after the type (`feat!: drop legacy endpoint`) or add a `BREAKING CHANGE:` footer.  This produces a **major** version bump regardless of the type prefix.

---

## Normal release workflow

### Step 1 — write conventional commits

Write commit messages using the prefixes above.  Example:

```
feat: add multi-contract SSE multiplexing endpoint

Adds GET /v1/events/stream/multi?contract_ids=C1,C2,C3.
Each event in the stream includes a `contract_id` field so clients
can demultiplex.

Closes #1050
```

### Step 2 — merge your feature PR

Merge the PR to `main` as normal.  release-please watches every push to `main`.

### Step 3 — release-please opens or updates a Release PR

After one or more conventional commits land on `main`, release-please:

1. Reads the commit log since the last tag.
2. Determines the next semver version.
3. Opens (or force-updates) a PR titled `chore: release X.Y.Z`.
4. The PR body contains a preview of the CHANGELOG entries and the version bump in `Cargo.toml` and `helm/soroban-pulse/Chart.yaml`.

The PR is **automatically kept up-to-date** — more merges to `main` will add new changelog entries to the same open Release PR.

### Step 4 — review and merge the Release PR

1. Review the generated CHANGELOG entries and version number.
2. If anything looks wrong (e.g. a missing entry or incorrect bump type), do **not** edit the PR directly.  Instead, push a fixup commit to `main` with the correct conventional-commit prefix and let release-please update the PR.
3. When satisfied, approve and merge the Release PR.

### Step 5 — automated publish (no action required)

Merging the Release PR triggers release-please to:
- Create the GitHub Release.
- Push the semver tag (`v1.2.3`) to the repository.

The tag push triggers `.github/workflows/release.yml`, which:

| Job | What it does |
|-----|-------------|
| `build-binaries` | Builds `soroban-pulse` for `linux/amd64`, `linux/arm64`, `darwin/amd64`, `darwin/arm64` using the Rust stable toolchain.  Each binary gets a SLSA provenance attestation. |
| `build-image` | Builds a multi-arch container image (`linux/amd64` + `linux/arm64`) and pushes it to GHCR with `vX.Y.Z`, `X.Y`, and `latest` tags.  Generates CycloneDX (crate dependency graph) and SPDX (container) SBOMs, then attests both. |
| `extract-openapi` | Uploads the pre-generated `openapi.json` as a release artifact. |
| `publish-release` | Downloads all artifacts, produces a `SHA256SUMS` file, and creates the GitHub Release with the full body (image pull commands, binary table, SBOM/checksum links, and `gh attestation verify` instructions). |

---

## Container image tags

After a stable release the following tags are available in GHCR:

```bash
ghcr.io/soroban-pulse/sorobanpulse:1.2.3   # exact version
ghcr.io/soroban-pulse/sorobanpulse:1.2      # latest patch of 1.2.x
ghcr.io/soroban-pulse/sorobanpulse:latest   # latest stable release
```

Pre-releases (e.g. `v1.2.3-rc.1`) are pushed with the exact tag only — `latest` is **not** updated.

---

## Required secrets and permissions

The release workflows use only the built-in `GITHUB_TOKEN` — no extra secrets are needed.

The token needs the following permissions (already configured in the workflow files):

| Permission | Required by |
|------------|------------|
| `contents: write` | Creating GitHub Releases and tags |
| `packages: write` | Pushing images to GHCR |
| `pull-requests: write` | release-please opening/updating Release PRs |
| `id-token: write` | Sigstore/OIDC for SLSA attestations |
| `attestations: write` | `actions/attest-build-provenance` and `actions/attest-sbom` |

---

## Verifying release artefacts

### Binary provenance

```bash
# Download the binary from the GitHub Release, then:
gh attestation verify soroban-pulse-linux-amd64 \
  --repo Soroban-Pulse/SorobanPulse
```

### Container image provenance

```bash
gh attestation verify oci://ghcr.io/soroban-pulse/sorobanpulse:1.2.3 \
  --repo Soroban-Pulse/SorobanPulse
```

### Checksum verification

```bash
# Download SHA256SUMS from the GitHub Release, then:
sha256sum --check SHA256SUMS
```

---

## How versions are tracked

Two files are updated by release-please on each release:

| File | Field | Example |
|------|-------|---------|
| `Cargo.toml` | `[package].version` | `1.2.3` |
| `helm/soroban-pulse/Chart.yaml` | `appVersion` | `"1.2.3"` |

The mapping is defined in `.release-please-config.json` under `extra-files`.

The manifest file `.release-please-manifest.json` records the version of the last release and is committed by release-please as part of the Release PR.  **Do not edit it manually.**

---

## Helm chart versions

The Helm chart has two independent version fields:

| Field | Managed by | Purpose |
|-------|-----------|---------|
| `version` | Manual / chart maintainer | Chart API version (bump when chart templates change) |
| `appVersion` | release-please | Application version (always mirrors `Cargo.toml`) |

When you make changes to the Helm templates themselves (not the application code), bump `version` manually in `helm/soroban-pulse/Chart.yaml` as part of your PR.

---

## Cutting a hotfix release

If a critical fix must ship outside the normal release-please flow:

1. Branch from the latest release tag:
   ```bash
   git checkout -b hotfix/1.2.4 v1.2.3
   ```
2. Apply the fix with a `fix:` commit.
3. Open a PR targeting `main` (and cherry-pick to a maintenance branch if needed).
4. After merging, release-please will incorporate the fix into its next Release PR.

For an *immediate* tag without waiting for release-please, push a semver tag directly:

```bash
git tag v1.2.4
git push origin v1.2.4
```

This triggers `release.yml` directly (it responds to any `v*.*.*` tag), producing all artefacts.  You will need to manually update `Cargo.toml`, `Chart.yaml`, and `CHANGELOG.md` before pushing the tag, and then run release-please manually or let it open a follow-up cleanup PR.

---

## Troubleshooting

### release-please is not opening a PR

- Ensure at least one `feat:`, `fix:`, or other conventional-commit with a non-hidden section has been merged since the last release.  `ci:`, `chore:`, and `test:` commits are hidden and do not trigger a Release PR on their own.
- Check the **Actions** tab for the `Release Please` workflow run for error details.
- Make sure the `GITHUB_TOKEN` has `pull-requests: write` and `contents: write` (see repo settings → Actions → General → Workflow permissions → "Read and write permissions").

### release-please PR has the wrong version

Correct the commit that caused the wrong bump:
- If a `fix:` should have been `feat:`, push a new `feat!: ...` commit (breaking change) or an empty `feat: ...` commit to force a re-evaluation.  Then close and re-open the Release PR — release-please will regenerate it.

### Container image build fails

- Check the `build-image` job logs under the `Release` workflow run.
- Buildx multi-arch builds require QEMU; the workflow sets it up automatically.
- Ensure the GHCR package visibility is set to Public or that `GITHUB_TOKEN` has `packages: write`.

### Attestation / SLSA step fails

`actions/attest-build-provenance` requires `id-token: write` and `attestations: write` permissions, and only works on GitHub-hosted runners.  Self-hosted runners need OIDC configured separately.

---

[release-please]: https://github.com/googleapis/release-please
[Conventional Commits]: https://www.conventionalcommits.org/en/v1.0.0/
