# AWS Deployment Runbook (EC2 + RDS + ALB)

Deploys SorobanPulse as a Docker container on a self-managed **EC2** instance
inside a VPC you provision, backed by **RDS PostgreSQL** and fronted by an
**Application Load Balancer** terminating TLS with an ACM certificate. This is
the infrastructure-level alternative to the managed
[AWS ECS Fargate guide](../deployment-platforms.md#1-aws-ecs-fargate) — use
this runbook when you need control over the host (custom AMI, kernel tuning,
sidecar processes) rather than a serverless container platform.

## Prerequisites

- **Accounts** — an AWS account with permission to create VPC, EC2, RDS, ELBv2,
  ACM, and IAM resources.
- **CLI tools** — `aws` CLI v2 (`aws --version`), `docker` (only needed if you
  push a custom image to ECR instead of building on the instance), `psql` for
  verification.
- **Credentials** — `aws configure` (or an assumed role) with a profile that
  has the permissions above.
- **DNS** — a domain/subdomain you control, to point at the ALB and to issue
  the ACM certificate against (e.g. `pulse.example.com`).
- **Repository assets used by this runbook**:
  - Root [`Dockerfile`](../../Dockerfile) — builds the `soroban-pulse` image
    (exposes port `3000`, health-checks `/healthz/ready`).
  - [`.env.example`](../../.env.example) — full environment variable reference.
  - `terraform/modules/vpc`, `terraform/modules/rds`, and
    `terraform/modules/alb` — this repo's existing Terraform modules for
    networking, the database, and the load balancer are **compute-agnostic**
    (they don't assume Fargate) and can be reused as-is. Only
    `terraform/modules/ecs` is Fargate-specific and is **not** used here — the
    EC2 instance itself is provisioned directly via the `aws` CLI below,
    since this repo does not ship an EC2 Terraform module.

## Architecture

```
                              Internet
                                 │
                         ACM cert (TLS)
                                 │
                     ┌───────────────────────┐
                     │  Application Load      │   public subnets
                     │  Balancer (HTTPS:443)  │   (one per AZ)
                     └───────────┬────────────┘
                                 │ HTTP:3000 (target group health check
                                 │            GET /healthz/ready)
                     ┌───────────▼────────────┐
                     │  EC2 instance           │   private subnet
                     │  docker run soroban-pulse│
                     │  :3000                  │
                     └───────────┬────────────┘
                                 │ 5432
                     ┌───────────▼────────────┐
                     │  RDS PostgreSQL 16      │   private subnet
                     │  (Multi-AZ optional)    │
                     └─────────────────────────┘
```

- The ALB sits in **public subnets** and is the only internet-facing
  component; it terminates TLS using an ACM certificate (see
  [docs/deployment.md § TLS Termination](../deployment.md#tls-termination) for
  the general nginx/Caddy/ALB comparison — this runbook implements the ALB
  option in full).
- The EC2 instance and RDS both live in **private subnets** with no public IP;
  egress (for pulling the Docker image and calling the Soroban RPC endpoint)
  goes through a NAT Gateway.
- Security groups enforce least privilege: ALB SG → app SG on 3000; app SG →
  RDS SG on 5432. Nothing else can reach the app or the database directly.

## Deployment Steps

### 1. Provision networking, database, and load balancer with Terraform

Reuse the existing modules rather than hand-rolling VPC/RDS/ALB resources.
From `terraform/`:

```bash
cd terraform
terraform init
terraform workspace select production || terraform workspace new production

terraform apply \
  -var="environment=production" \
  -var="aws_region=us-east-1" \
  -var="certificate_arn=arn:aws:acm:us-east-1:ACCOUNT_ID:certificate/CERT_ID" \
  -target=module.vpc \
  -target=module.rds \
  -target=module.alb
```

> The `certificate_arn` variable must reference an ACM certificate already
> validated for your domain (`aws acm request-certificate --domain-name
> pulse.example.com --validation-method DNS`, then add the returned CNAME to
> your DNS zone and wait for `Status: ISSUED`).

Capture the outputs you'll need next:

```bash
terraform output -raw vpc_private_subnet_ids
terraform output -raw app_security_group_id
terraform output -raw rds_endpoint
terraform output -raw alb_target_group_arn
```

### 2. Build and publish the image

```bash
REGION=us-east-1
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
ECR_REPO="${ACCOUNT_ID}.dkr.ecr.${REGION}.amazonaws.com/soroban-pulse"

aws ecr create-repository --repository-name soroban-pulse --region "$REGION" || true
aws ecr get-login-password --region "$REGION" \
  | docker login --username AWS --password-stdin "${ACCOUNT_ID}.dkr.ecr.${REGION}.amazonaws.com"

docker build -t soroban-pulse .
docker tag soroban-pulse:latest "${ECR_REPO}:latest"
docker push "${ECR_REPO}:latest"
```

### 3. Create the EC2 security group

```bash
VPC_ID=$(terraform -chdir=terraform output -raw vpc_id)
ALB_SG_ID=$(terraform -chdir=terraform output -raw alb_security_group_id)

APP_SG_ID=$(aws ec2 create-security-group \
  --group-name soroban-pulse-app \
  --description "SorobanPulse app instance" \
  --vpc-id "$VPC_ID" \
  --query GroupId --output text)

# Only the ALB may reach the app on 3000
aws ec2 authorize-security-group-ingress \
  --group-id "$APP_SG_ID" \
  --protocol tcp --port 3000 \
  --source-group "$ALB_SG_ID"
```

### 4. Launch the EC2 instance with user-data

Use Amazon Linux 2023 (ships with `docker` in its repos, no third-party AMI
needed). `t3.small` (2 GiB RAM) is the minimum comfortable size for the app
container plus OS overhead; scale to `t3.medium` under sustained load.

```bash
cat > user-data.sh <<'EOF'
#!/bin/bash
set -euxo pipefail
dnf install -y docker
systemctl enable --now docker

aws ecr get-login-password --region ${REGION} \
  | docker login --username AWS --password-stdin ${ECR_REPO%/*}

docker pull ${ECR_REPO}:latest

cat > /etc/soroban-pulse.env <<ENV
DATABASE_URL=postgres://soroban_admin:CHANGE_ME@${RDS_ENDPOINT}:5432/soroban_pulse
STELLAR_RPC_URL=https://soroban-testnet.stellar.org
PORT=3000
RUST_LOG=info
RUST_LOG_FORMAT=json
DB_MAX_CONNECTIONS=10
RATE_LIMIT_PER_MINUTE=60
START_LEDGER=0
ENV

docker run -d --name soroban-pulse --restart unless-stopped \
  --env-file /etc/soroban-pulse.env \
  -p 3000:3000 \
  ${ECR_REPO}:latest
EOF

PRIVATE_SUBNET_ID=$(terraform -chdir=terraform output -json vpc_private_subnet_ids | jq -r '.[0]')

aws ec2 run-instances \
  --image-id resolve:ssm:/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-x86_64 \
  --instance-type t3.small \
  --subnet-id "$PRIVATE_SUBNET_ID" \
  --security-group-ids "$APP_SG_ID" \
  --iam-instance-profile Name=soroban-pulse-ec2-profile \
  --user-data file://user-data.sh \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=soroban-pulse-app}]' \
  --count 1
```

> RDS credentials should come from the Secrets Manager secret the `rds`
> module already creates (`terraform output -raw rds_secret_arn`) rather than
> being hardcoded in user-data — fetch it with `aws secretsmanager
> get-secret-value` inside `user-data.sh` in a production rollout. The inline
> value above is shown for clarity only.

### 5. Register the instance with the ALB target group

```bash
INSTANCE_ID=$(aws ec2 describe-instances \
  --filters "Name=tag:Name,Values=soroban-pulse-app" "Name=instance-state-name,Values=running" \
  --query "Reservations[0].Instances[0].InstanceId" --output text)

TG_ARN=$(terraform -chdir=terraform output -raw alb_target_group_arn)

aws elbv2 register-targets \
  --target-group-arn "$TG_ARN" \
  --targets Id="$INSTANCE_ID",Port=3000
```

The target group's health check is already configured by the `alb` module to
hit `/healthz/ready` (see `terraform/variables.tf` → `health_check_path`)
every 30 seconds with a 2-success threshold.

## Verification

```bash
# 1. Target is healthy in the ALB
aws elbv2 describe-target-health --target-group-arn "$TG_ARN"
# Expect: "State": "healthy"

# 2. Health endpoint reachable through the ALB (not localhost)
ALB_DNS=$(terraform -chdir=terraform output -raw alb_dns_name)
curl -sf "https://${ALB_DNS}/healthz/ready" | jq .
# Expect: {"status":"ok","db":"ok","indexer":"ok"}

curl -sf "https://${ALB_DNS}/healthz/live"

# 3. Smoke-test a real API endpoint
curl -sf "https://${ALB_DNS}/v1/events?limit=1" | jq .

# 4. RDS is reachable from inside the VPC (run from the EC2 instance via SSM)
aws ssm start-session --target "$INSTANCE_ID"
#   then, on the instance:
psql "$DATABASE_URL" -c "SELECT 1;"

# 5. Indexer is making progress
curl -sf "https://${ALB_DNS}/metrics" | grep soroban_pulse_indexer_current_ledger
```

## Rollback

- **Bad app deploy**: pull and run the previous image tag, then re-register:
  ```bash
  docker pull "${ECR_REPO}:<previous-tag>"
  docker stop soroban-pulse && docker rm soroban-pulse
  docker run -d --name soroban-pulse --restart unless-stopped \
    --env-file /etc/soroban-pulse.env -p 3000:3000 "${ECR_REPO}:<previous-tag>"
  ```
  Or, for a fleet, launch a new instance from the previous AMI/user-data and
  swap it into the target group before deregistering the bad one
  (zero-downtime).
- **Bad DB migration**: SorobanPulse applies migrations automatically on
  startup. Follow the migration rollback procedure in
  [docs/deployment.md § Migration Strategy](../deployment.md#migration-strategy)
  before rolling the app back, so the old binary isn't pointed at a newer
  schema.
- **Abandoning the deployment**: deregister the target
  (`aws elbv2 deregister-targets`), terminate the instance
  (`aws ec2 terminate-instances`), then `terraform destroy -target=module.alb
  -target=module.rds -target=module.vpc` if the networking/DB are no longer
  needed. Take a final RDS snapshot first if any data must be retained.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| Target group shows `unhealthy` | Security group blocks ALB→app on 3000, or app isn't listening yet | `aws ec2 describe-security-groups --group-ids $APP_SG_ID`; confirm the ingress rule's source is the ALB SG; check `docker logs soroban-pulse` on the instance for a crash loop |
| ALB returns `502 Bad Gateway` | App container crashed or is still starting (30 s `start_period` in the Dockerfile healthcheck) | `docker ps -a` on the instance; `docker logs soroban-pulse --tail 100` |
| ALB returns `504 Gateway Timeout` | App is up but slow to respond (DB latency, connection pool exhaustion) | See [docs/runbooks/db-pool-exhaustion.md](../runbooks/db-pool-exhaustion.md) |
| App can't reach RDS (`connection refused`/timeout) | RDS security group doesn't allow the app SG on 5432, or the instance is in the wrong subnet | `aws ec2 describe-security-groups --group-ids <rds-sg-id>`; confirm ingress source is `$APP_SG_ID` |
| `docker pull` fails on instance with `no basic auth credentials` | ECR login token expired (12 h) or the instance profile lacks `ecr:GetAuthorizationToken` | Re-run the `aws ecr get-login-password` step; check the IAM instance profile's attached policy |
| ACM certificate stuck in `PENDING_VALIDATION` | DNS validation CNAME not created, or created in the wrong zone | `aws acm describe-certificate --certificate-arn ...`; re-check the `ResourceRecord` against your DNS zone |
| High connection count on RDS | `DB_MAX_CONNECTIONS` too high relative to instance class, or a leak | See [docs/runbooks/db-pool-exhaustion.md](../runbooks/db-pool-exhaustion.md); `db.t3.medium` allows ~180 connections — leave headroom for `psql`/monitoring |

For anything beyond the deployment itself (indexer lag, RPC errors, webhook
failures once the service is live), use
[docs/runbooks/operator-runbook.md](../runbooks/operator-runbook.md).

### Cost note

`t3.small` (~$15/mo) + `db.t3.medium` RDS single-AZ (~$50/mo) + ALB
(~$20/mo) + NAT Gateway (~$33/mo) ≈ **$120/mo** before data transfer — notably
more than the ECS Fargate estimate in
[docs/deployment-platforms.md](../deployment-platforms.md#cost-estimate-us-east-1-2026)
because you're paying for the NAT Gateway and a always-on EC2 instance rather
than metered Fargate task time. Use a single NAT Gateway
(`single_nat_gateway = true` in `terraform/variables.tf`) to cut that cost in
non-production environments.
