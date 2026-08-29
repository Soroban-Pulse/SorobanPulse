# Container Image Scanning and Signing (Issue #907)

SorobanPulse's container image supply chain is secured in [`.github/workflows/docker-publish.yml`](../.github/workflows/docker-publish.yml), which runs on every push to `main` and on version tags (`v*.*.*`). This document explains the pipeline stages and how to consume their outputs.

## Pipeline Overview

```
checkout → buildx build (provenance + sbom attestations)
         → Trivy vulnerability scan  → SARIF → GitHub code scanning
         → SPDX SBOM generation      → uploaded as build artifact (90-day retention)
         → cosign keyless signing    → signature stored in the GHCR registry (sigstore/Rekor)
```

The job requires `packages: write`, `security-events: write`, and `id-token: write` permissions — the last one is what enables cosign's keyless (OIDC) signing without storing a private key as a secret.

## Vulnerability Scanning (Trivy)

- Tool: [`aquasecurity/trivy-action`](https://github.com/aquasecurity/trivy-action) v0.28.0.
- Scans the exact image digest that was just built and pushed (`image-ref: <name>@<digest>`), not a re-pulled tag — avoids TOCTOU gaps between build and scan.
- Severity filter: `CRITICAL,HIGH`.
- Output format: SARIF, uploaded via `github/codeql-action/upload-sarif` so results appear under the repo's **Security → Code scanning alerts** tab.
- `exit-code: "0"` — **the scan is currently informational only and does not fail the build.** Findings are visible in the Security tab but a critical CVE will not block a merge or a release tag today.

### Viewing results

```bash
gh api repos/{owner}/{repo}/code-scanning/alerts --jq '.[] | {rule: .rule.id, severity: .rule.security_severity_level, state: .state}'
```

Or browse **Security → Code scanning alerts** in the GitHub UI, filtered by tool `Trivy`.

## SBOM Generation

Two SBOMs are produced per build:

1. **Build-time provenance/SBOM attestation** — `docker/build-push-action` is run with `provenance: true` and `sbom: true`, attaching in-toto attestations to the pushed image manifest.
2. **Standalone SPDX SBOM** — [`anchore/sbom-action`](https://github.com/anchore/sbom-action) generates `sbom.spdx.json` for the built image and it's uploaded as a workflow artifact named `sbom-<git-sha>`, retained for 90 days.

### Retrieving an SBOM

```bash
# From a workflow run
gh run download <run-id> -n sbom-<git-sha>

# Or pull the in-toto attestation directly from the registry
cosign download sbom ghcr.io/<owner>/<repo>@<digest>
```

## Image Signing (Cosign)

- Tool: [`sigstore/cosign-installer`](https://github.com/sigstore/cosign-installer) v3, using **keyless signing** (`COSIGN_EXPERIMENTAL=1`) — the signature is bound to the GitHub Actions OIDC identity, with no long-lived signing key to manage or rotate.
- Signs the exact digest produced by the build step: `cosign sign --yes <image>@<digest>`.
- The signature and its Rekor transparency-log entry are public — anyone can verify an image actually came from this repository's `docker-publish.yml` workflow.

### Verifying a signed image

```bash
cosign verify \
  --certificate-identity-regexp "https://github.com/<owner>/<repo>/.github/workflows/docker-publish.yml@.*" \
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \
  ghcr.io/<owner>/<repo>@<digest>
```

A verification failure means the image either wasn't built by this workflow or was tampered with after signing — treat it as a supply-chain incident, not a config error to work around.

## Policy Enforcement

Policy enforcement is the main outstanding checklist item. Today, scanning and signing are **advisory** — nothing in CI or in deployment currently rejects an unscanned, vulnerable, or unsigned image. To close this gap:

- **Blocking scans**: change `exit-code: "0"` to `"1"` in the Trivy step (or add a separate gate job) once the team is ready to treat new CRITICAL/HIGH CVEs as merge-blocking, and establish a documented exception/allowlist process (e.g., a `.trivyignore` with expiry-dated entries) so a single unfixable upstream CVE doesn't permanently block releases.
- **Admission-time verification**: enforce `cosign verify` at deploy time (e.g., a Kubernetes admission controller such as [Sigstore Policy Controller](https://docs.sigstore.dev/policy-controller/overview/) or an ECS/Fargate pre-deploy check) so only signed images built by `docker-publish.yml` can run in production — this is what actually makes signing security-relevant rather than informational.
- **SBOM policy**: gate on SBOM presence/format before promoting an image to a production tag.

## Image Retention

[`.github/workflows/container-retention.yml`](../.github/workflows/container-retention.yml) runs weekly (Sunday 03:00 UTC) and prunes **untagged** GHCR image versions (builds superseded by a later push with no branch/sha/semver tag pointing at them), keeping at least 20 versions. This is unrelated to security scanning but keeps the registry from accumulating scanned-but-orphaned images indefinitely.

## Testing the Workflow

- Trigger a manual run against a branch by pushing a change under one of the watched paths (`src/**`, `bin/**`, `migrations/**`, `Cargo.toml`, `Cargo.lock`, `Dockerfile`, `.dockerignore`).
- Confirm all three artifacts land: a SARIF entry under Security → Code scanning, an `sbom-<sha>` workflow artifact, and a `cosign verify` success against the pushed digest.
- To test policy enforcement changes locally before wiring them into CI, run Trivy directly: `trivy image --severity CRITICAL,HIGH --exit-code 1 <image>`.

## Related Documentation

- [`.github/workflows/docker-publish.yml`](../.github/workflows/docker-publish.yml) — pipeline source of truth
- [deployment.md](deployment.md) / [deployment-platforms.md](deployment-platforms.md) — where built images are deployed
- [owasp_security_headers.md](owasp_security_headers.md) — related application-layer security hardening
