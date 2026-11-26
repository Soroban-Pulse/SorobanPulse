# Maintainers

This file lists the people responsible for reviewing and merging changes to
Soroban Pulse, and the areas each of them owns. See
[GOVERNANCE.md](GOVERNANCE.md) for what each role means and how to join.

The same ownership is encoded in [.github/CODEOWNERS](.github/CODEOWNERS), so
GitHub requests reviews from the right people automatically. **When you change
this file, update CODEOWNERS in the same pull request.**

## Maintainers

| Name / GitHub team | Areas |
|--------------------|-------|
| `@Soroban-Pulse/maintainers` | Project lead; governance, releases, CI, anything not listed below |
| `@Soroban-Pulse/backend` | Indexer, REST/GraphQL API, database migrations (`src/`, `migrations/`) |
| `@Soroban-Pulse/security` | Encryption, authentication, secrets, `SECURITY.md`, `deny.toml` |
| `@Soroban-Pulse/sdk` | SDKs and client packages (`sdk/`, `packages/`, `cli/`, `vscode-extension/`) |
| `@Soroban-Pulse/frontend` | Dashboard and web UI (`dashboard/`, `frontend/`, `web/`) |
| `@Soroban-Pulse/infra` | Deployment: Docker, Helm, Kubernetes, Terraform, GitOps |
| `@Soroban-Pulse/docs` | Documentation (`docs/`, top-level `*.md`) |

Individual maintainers are members of the teams above; see the
[organization's teams page](https://github.com/orgs/Soroban-Pulse/teams) for
current membership.

## Reviewers

Reviewers may approve pull requests in their areas but do not have merge
rights. Add yourself here via the process in [GOVERNANCE.md](GOVERNANCE.md).

| GitHub handle | Areas |
|---------------|-------|
| _none yet_ | |

## Automated Issue Rotation

The scheduled `fuzz.yml` and `soak-testnet.yml` workflows open issues labelled
`automated` (plus `fuzzing` or `soak-test`), and report build failures with the
`build-failure` label. These issues are triaged on a weekly rotation so that
real crashes — input-validation panics are a DoS vector — are not buried among
noise.

- **Who is on rotation:** one member of `@Soroban-Pulse/security` (fuzz-crash
  issues) and one member of `@Soroban-Pulse/infra` (soak-testnet failures),
  rotating weekly in the order listed in the organization's teams page.
- **Cadence:** the rotation hand-off happens every Monday; the outgoing owner
  must leave a comment on each open `automated` issue summarising its state.
- **Expectations for each issue:**
  1. Reproduce the crash or failure at the commit named in the issue.
  2. Add the minimised crashing input to the regression corpus
     (`fuzz/corpus/<target>/`) so the target no longer regresses.
  3. Fix the underlying bug, or close the issue as a duplicate (linking the
     original) or as a build failure (re-labelling it `build-failure`).
- The bots comment on an existing open issue for the same target or test
  instead of opening a new one; if you see duplicates anyway, close them and
  link the canonical issue.

If ownership of this rotation changes, update
[.github/CODEOWNERS](.github/CODEOWNERS) in the same pull request.

## Emeritus

We thank the following people for their past service as maintainers:

- _none yet_

## Contacting Maintainers

- Security issues: follow [SECURITY.md](SECURITY.md) — do **not** open a public issue.
- Code of Conduct concerns: see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md#enforcement).
- Everything else: open an issue or discussion and mention the relevant team.
