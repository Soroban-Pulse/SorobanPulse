# Deployment Runbooks

Step-by-step, infrastructure-level runbooks for deploying SorobanPulse
yourself on a cloud provider's raw compute/network primitives — you
provision and manage the VM, VPC/VNet, database instance, and load
balancer/gateway directly, rather than handing that layer to a managed
platform.

This is the counterpart to
[docs/deployment-platforms.md](../deployment-platforms.md), which covers
managed/serverless platforms (AWS ECS Fargate, Google Cloud Run,
DigitalOcean App Platform, Heroku, Railway) where the provider owns the
compute layer for you. Use **this** directory when you need control over the
host — custom AMIs/images, kernel or OS package tuning, on-prem/bare-metal
requirements, or simply no serverless option exists for your provider (Azure,
for example, has no entry in `deployment-platforms.md`).

## Runbooks

| Runbook | Compute | Database | Load balancer / TLS |
|---|---|---|---|
| [template.md](template.md) | — | — | Reference structure every runbook below follows |
| [aws.md](aws.md) | EC2 | RDS PostgreSQL | Application Load Balancer (ACM cert) |
| [gcp.md](gcp.md) | Compute Engine | Cloud SQL for PostgreSQL | Cloud Load Balancing (Google-managed cert) |
| [azure.md](azure.md) | Azure VM | Azure Database for PostgreSQL (Flexible Server) | Application Gateway (TLS) |
| [self-hosted.md](self-hosted.md) | Bare metal / on-prem, Docker Compose, or a systemd-managed container | Self-managed PostgreSQL | Your own nginx/Caddy — see [docs/deployment.md](../deployment.md) |

Each platform runbook follows the exact same section structure defined in
[template.md](template.md): **Prerequisites**, **Architecture**,
**Deployment Steps**, **Verification**, **Rollback**, and
**Troubleshooting**. [`scripts/check-deployment-runbooks.sh`](../../scripts/check-deployment-runbooks.sh)
enforces this structurally — see
[testing-framework.md](testing-framework.md) for what it checks and how to
run it.

## Related docs

- [docs/deployment-platforms.md](../deployment-platforms.md) — managed/serverless
  platforms (AWS ECS Fargate, Cloud Run, DigitalOcean App Platform, Heroku,
  Railway).
- [docs/deployment.md](../deployment.md) — general deployment guidance that
  applies regardless of platform: resource sizing, TLS termination options,
  horizontal scaling, migration strategy, secret management.
- [docs/multi-deployment-architecture.md](../multi-deployment-architecture.md) —
  multi-region/multi-deployment architecture, once a single-region deployment
  from one of the runbooks above is running.
- [docs/runbooks/](../runbooks/) — **operational** incident runbooks (DB pool
  exhaustion, indexer lag, RPC errors, webhook failures, etc.) for a system
  that is already deployed. Do not confuse these with the deployment runbooks
  in this directory — this directory gets you to a running deployment;
  `docs/runbooks/` keeps it running.
