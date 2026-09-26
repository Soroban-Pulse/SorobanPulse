# Runbook Template

This is the reference structure every runbook in this directory follows. When
adding a new platform runbook, copy this file, keep the section headings
**exactly as written** (the linter in
[`scripts/check-deployment-runbooks.sh`](../../scripts/check-deployment-runbooks.sh)
greps for them), and fill in platform-specific detail.

> Do not rename or remove `## Prerequisites`, `## Architecture`,
> `## Deployment Steps`, `## Verification`, `## Rollback`, or
> `## Troubleshooting` — the CI check fails the build if any is missing.

---

# <Platform Name> Deployment Runbook

One-sentence description of what this runbook deploys and the compute model
it uses (e.g. "a single EC2 instance behind an ALB, with RDS PostgreSQL" —
not a managed/serverless platform; see
[docs/deployment-platforms.md](../deployment-platforms.md) for those).

## Prerequisites

List everything that must exist *before* Step 1, so a reader can gather it
up front instead of discovering a missing credential halfway through:

- **Accounts** — cloud account/subscription, billing enabled.
- **CLI tools** — exact tool names and minimum versions (e.g. `aws` CLI v2,
  `gcloud`, `az`, `docker`, `psql`).
- **Credentials** — how the reader authenticates (`aws configure`,
  `gcloud auth login`, `az login`, an SSH key pair, etc.).
- **DNS** — a domain/subdomain the reader controls, if the runbook issues a
  TLS certificate against it.
- **Repository assets** — which files in this repo the runbook uses (the
  root `Dockerfile`, `docker-compose.yml`, `.env.example`, a `terraform/`
  module, etc.). Link to them; don't restate their contents.

## Architecture

A short paragraph plus a diagram (ASCII is fine) describing the network and
compute topology: which components are public vs. private, where TLS
terminates, and how traffic reaches the SorobanPulse app container/process.
Call out anything that differs from the general guidance in
[docs/deployment.md](../deployment.md) (TLS termination options, resource
sizing) and [docs/multi-deployment-architecture.md](../multi-deployment-architecture.md)
(multi-region topology) rather than repeating it.

## Deployment Steps

Numbered, copy-pasteable steps with concrete flag values (real machine
types/instance classes, real CIDR ranges, real health-check paths — not
placeholders like `<something>` unless the value is genuinely
account-specific, e.g. an account ID or ARN). Each step should be a fenced
code block runnable as-is after substituting account-specific values.

1. Step one.
2. Step two.
3. …

## Verification

Concrete commands that prove the deployment actually works — not just that
`terraform apply` or a CLI command returned `0`:

- A health-check request that reaches the app **through** the
  load balancer/gateway (not `localhost`), e.g.
  `curl -sf https://<public-endpoint>/healthz/ready`.
- A database connectivity check from the app's network context.
- A smoke-test API request exercising a real endpoint (e.g.
  `GET /v1/events?limit=1`).
- Where relevant, a metrics/log check confirming the indexer is making
  progress (`soroban_pulse_indexer_current_ledger` increasing).

## Rollback

How to undo this deployment or revert to the previous known-good version,
including:

- Rolling the app back to the previous image tag/binary/AMI.
- Whether the database migration needs a corresponding rollback (link to
  the "Migration rollback procedure" in [docs/deployment.md](../deployment.md#migration-strategy)
  rather than duplicating it).
- How to tear down infrastructure created in this runbook if abandoning the
  deployment entirely.

## Troubleshooting

A table or list of the failure modes specific to this platform (security
group/firewall/NSG misconfiguration, load-balancer health-check failures,
TLS certificate provisioning issues, database connectivity errors) with a
diagnostic command and a fix for each. For issues that are really
application/database runbook material (connection pool exhaustion, indexer
lag, RPC errors), link to the matching doc in
[docs/runbooks/](../runbooks/) instead of duplicating it:

- [docs/runbooks/db-pool-exhaustion.md](../runbooks/db-pool-exhaustion.md)
- [docs/runbooks/indexer-lag.md](../runbooks/indexer-lag.md)
- [docs/runbooks/rpc-errors.md](../runbooks/rpc-errors.md)
- [docs/runbooks/operator-runbook.md](../runbooks/operator-runbook.md) for
  general incident response once the platform-level deployment is healthy.
