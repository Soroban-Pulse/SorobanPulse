# Deployment Guide: Common Platforms

Step-by-step guides for deploying SorobanPulse on major cloud providers and
platforms, with environment configuration, cost estimates, and platform-specific
notes.

> **TLS note:** All production deployments should sit behind a TLS-terminating
> reverse proxy or load balancer.  See [docs/deployment.md](deployment.md) for
> nginx, Caddy, and AWS ALB termination options.

> **Provisioning your own VM/network layer instead?** This guide covers
> managed/serverless platforms only.  For full VM/infrastructure-level
> runbooks (EC2/RDS/ALB, Compute Engine/Cloud SQL, Azure VMs/App Gateway,
> self-hosted) see [docs/deployment-runbooks/README.md](deployment-runbooks/README.md).

---

## Table of Contents

1. [AWS ECS (Fargate)](#1-aws-ecs-fargate)
2. [Google Cloud Run](#2-google-cloud-run)
3. [DigitalOcean App Platform](#3-digitalocean-app-platform)
4. [Heroku](#4-heroku)
5. [Railway](#5-railway)
6. [Environment variable reference](#environment-variable-reference)
7. [Cost summary](#cost-summary)

---

## 1. AWS ECS (Fargate)

Run SorobanPulse as a serverless container on AWS Fargate with RDS PostgreSQL.

### Prerequisites

- AWS CLI v2 configured (`aws configure`)
- Docker installed locally
- An Amazon ECR repository
- An RDS PostgreSQL 16 instance (or Aurora PostgreSQL-compatible)

### Step 1 — Build and push the image

```bash
REGION=us-east-1
ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
ECR_REPO="${ACCOUNT_ID}.dkr.ecr.${REGION}.amazonaws.com/soroban-pulse"

# Create ECR repo (once)
aws ecr create-repository --repository-name soroban-pulse --region "$REGION"

# Authenticate Docker
aws ecr get-login-password --region "$REGION" \
  | docker login --username AWS --password-stdin "${ACCOUNT_ID}.dkr.ecr.${REGION}.amazonaws.com"

# Build and push
docker build -t soroban-pulse .
docker tag soroban-pulse:latest "${ECR_REPO}:latest"
docker push "${ECR_REPO}:latest"
```

### Step 2 — Store secrets in AWS Secrets Manager

```bash
aws secretsmanager create-secret \
  --name soroban-pulse/prod \
  --region "$REGION" \
  --secret-string '{
    "DATABASE_URL":    "postgres://user:pass@rds-endpoint:5432/soroban_pulse",
    "API_KEY":         "your-api-key",
    "ADMIN_API_KEY":   "your-admin-key"
  }'
```

### Step 3 — Create an ECS task definition

Save as `ecs-task-definition.json`:

```json
{
  "family": "soroban-pulse",
  "networkMode": "awsvpc",
  "requiresCompatibilities": ["FARGATE"],
  "cpu": "512",
  "memory": "1024",
  "executionRoleArn": "arn:aws:iam::ACCOUNT_ID:role/ecsTaskExecutionRole",
  "containerDefinitions": [
    {
      "name": "soroban-pulse",
      "image": "ACCOUNT_ID.dkr.ecr.REGION.amazonaws.com/soroban-pulse:latest",
      "portMappings": [{ "containerPort": 3000, "protocol": "tcp" }],
      "environment": [
        { "name": "PORT",                         "value": "3000" },
        { "name": "RUST_LOG",                     "value": "info" },
        { "name": "RUST_LOG_FORMAT",              "value": "json" },
        { "name": "START_LEDGER",                 "value": "0" },
        { "name": "STELLAR_RPC_URL",              "value": "https://soroban-testnet.stellar.org" },
        { "name": "DB_MAX_CONNECTIONS",           "value": "10" },
        { "name": "RATE_LIMIT_PER_MINUTE",        "value": "60" },
        { "name": "SSE_KEEPALIVE_SECS",           "value": "15" },
        { "name": "INDEXER_LOCK_RETRY_SECS",      "value": "30" }
      ],
      "secrets": [
        {
          "name": "DATABASE_URL",
          "valueFrom": "arn:aws:secretsmanager:REGION:ACCOUNT_ID:secret:soroban-pulse/prod:DATABASE_URL::"
        },
        {
          "name": "API_KEY",
          "valueFrom": "arn:aws:secretsmanager:REGION:ACCOUNT_ID:secret:soroban-pulse/prod:API_KEY::"
        },
        {
          "name": "ADMIN_API_KEY",
          "valueFrom": "arn:aws:secretsmanager:REGION:ACCOUNT_ID:secret:soroban-pulse/prod:ADMIN_API_KEY::"
        }
      ],
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group":         "/ecs/soroban-pulse",
          "awslogs-region":        "REGION",
          "awslogs-stream-prefix": "ecs"
        }
      },
      "healthCheck": {
        "command":     ["CMD-SHELL", "curl -f http://localhost:3000/healthz/ready || exit 1"],
        "interval":    10,
        "timeout":     5,
        "retries":     3,
        "startPeriod": 30
      }
    }
  ]
}
```

Register the task definition:

```bash
aws ecs register-task-definition \
  --region "$REGION" \
  --cli-input-json file://ecs-task-definition.json
```

### Step 4 — Create ECS service with ALB

```bash
# Create an ECS cluster
aws ecs create-cluster --cluster-name soroban-pulse --region "$REGION"

# Create the service (assumes VPC, subnets, ALB target group already configured)
aws ecs create-service \
  --cluster soroban-pulse \
  --service-name soroban-pulse-svc \
  --task-definition soroban-pulse \
  --desired-count 2 \
  --launch-type FARGATE \
  --network-configuration "awsvpcConfiguration={
    subnets=[subnet-aaaa,subnet-bbbb],
    securityGroups=[sg-xxxx],
    assignPublicIp=DISABLED}" \
  --load-balancers "targetGroupArn=arn:aws:elasticloadbalancing:...,
    containerName=soroban-pulse,containerPort=3000" \
  --region "$REGION"
```

### Step 5 — Multi-replica advisory lock

With `--desired-count 2`, both replicas start an indexer.  The advisory lock in
`indexer.rs` ensures only one replica indexes at a time:

- Set `INDEXER_LOCK_RETRY_SECS=30` (default) on all replicas.
- The replica that acquires the lock indexes; the other is in standby.
- If the leader task crashes, ECS replaces it and the standby promotes within
  one retry interval.

### Step 6 — Auto-scaling

```bash
aws application-autoscaling register-scalable-target \
  --service-namespace ecs \
  --resource-id service/soroban-pulse/soroban-pulse-svc \
  --scalable-dimension ecs:service:DesiredCount \
  --min-capacity 1 \
  --max-capacity 10

aws application-autoscaling put-scaling-policy \
  --service-namespace ecs \
  --resource-id service/soroban-pulse/soroban-pulse-svc \
  --scalable-dimension ecs:service:DesiredCount \
  --policy-name soroban-pulse-cpu-scaling \
  --policy-type TargetTrackingScaling \
  --target-tracking-scaling-policy-configuration '{
    "TargetValue": 70.0,
    "PredefinedMetricSpecification": {
      "PredefinedMetricType": "ECSServiceAverageCPUUtilization"
    }
  }'
```

### ECS environment variables

```
DATABASE_URL          = postgres://user:pass@rds-host:5432/soroban_pulse
STELLAR_RPC_URL       = https://soroban-testnet.stellar.org
PORT                  = 3000
RUST_LOG              = info
RUST_LOG_FORMAT       = json
DB_MAX_CONNECTIONS    = 10
RATE_LIMIT_PER_MINUTE = 60
START_LEDGER          = 0
INDEXER_LOCK_RETRY_SECS = 30
```

### Cost estimate (us-east-1, 2026)

| Resource | Spec | Est. monthly cost |
|---|---|---|
| Fargate (2 tasks) | 0.5 vCPU / 1 GB each, ~730 h | ~$30 |
| RDS PostgreSQL | `db.t4g.micro`, 20 GB gp3 | ~$15 |
| ALB | 1 LCU average | ~$20 |
| ECR storage | < 1 GB | ~$0.10 |
| CloudWatch logs | ~10 GB/mo | ~$5 |
| **Total** | | **~$70/mo** |

> Costs vary by region and traffic.  Use the [AWS Pricing Calculator](https://calculator.aws/) for accurate estimates.

---

## 2. Google Cloud Run

Fully managed serverless containers with automatic scaling to zero.

### Prerequisites

- `gcloud` CLI installed and authenticated
- A Google Cloud project with billing enabled
- Cloud SQL PostgreSQL instance (or Cloud SQL Auth Proxy)

### Step 1 — Build and push to Artifact Registry

```bash
PROJECT_ID=$(gcloud config get-value project)
REGION=us-central1
IMAGE="$REGION-docker.pkg.dev/$PROJECT_ID/soroban-pulse/app"

# Create repository (once)
gcloud artifacts repositories create soroban-pulse \
  --repository-format=docker \
  --location="$REGION"

# Build and push
docker build -t "$IMAGE:latest" .
docker push "$IMAGE:latest"
```

### Step 2 — Store secrets in Secret Manager

```bash
echo -n "postgres://user:pass@/soroban_pulse?host=/cloudsql/PROJECT:REGION:INSTANCE" \
  | gcloud secrets create DATABASE_URL --data-file=-

echo -n "your-api-key" | gcloud secrets create SOROBAN_API_KEY --data-file=-
echo -n "your-admin-key" | gcloud secrets create SOROBAN_ADMIN_KEY --data-file=-
```

### Step 3 — Deploy to Cloud Run

```bash
gcloud run deploy soroban-pulse \
  --image "$IMAGE:latest" \
  --region "$REGION" \
  --platform managed \
  --min-instances 1 \
  --max-instances 10 \
  --cpu 1 \
  --memory 512Mi \
  --timeout 300 \
  --concurrency 80 \
  --port 3000 \
  --add-cloudsql-instances "$PROJECT_ID:$REGION:soroban-pulse-db" \
  --set-env-vars "PORT=3000,RUST_LOG=info,RUST_LOG_FORMAT=json,DB_MAX_CONNECTIONS=10,RATE_LIMIT_PER_MINUTE=60,STELLAR_RPC_URL=https://soroban-testnet.stellar.org,INDEXER_LOCK_RETRY_SECS=30" \
  --set-secrets "DATABASE_URL=DATABASE_URL:latest,API_KEY=SOROBAN_API_KEY:latest,ADMIN_API_KEY=SOROBAN_ADMIN_KEY:latest" \
  --allow-unauthenticated
```

### Step 4 — Health check and liveness probes

Cloud Run uses the HTTP health check automatically.  Ensure your service
account has `roles/cloudsql.client`.

```bash
gcloud run services describe soroban-pulse --region "$REGION" \
  --format "value(status.url)"
```

### Step 5 — Custom domain (optional)

```bash
gcloud beta run domain-mappings create \
  --service soroban-pulse \
  --domain api.yourproject.com \
  --region "$REGION"
```

### Cloud Run environment variables

```
DATABASE_URL          = postgres://user:pass@/soroban_pulse?host=/cloudsql/...
STELLAR_RPC_URL       = https://soroban-testnet.stellar.org
PORT                  = 3000
RUST_LOG              = info
RUST_LOG_FORMAT       = json
DB_MAX_CONNECTIONS    = 10
RATE_LIMIT_PER_MINUTE = 60
START_LEDGER          = 0
INDEXER_LOCK_RETRY_SECS = 30
```

### Cloud Run notes

- **Scale to zero** is enabled by default.  Set `--min-instances 1` to avoid
  cold starts if you need the indexer running continuously.
- The SSE endpoint requires HTTP/2 or a client that tolerates long-lived HTTP/1.1
  connections.  Cloud Run supports both.
- The advisory lock still works correctly when multiple instances are deployed —
  only one indexes at a time.

### Cost estimate (us-central1, 2026)

| Resource | Spec | Est. monthly cost |
|---|---|---|
| Cloud Run | 1 vCPU / 512 MB, 1 min instance, ~2M req/mo | ~$10 |
| Cloud SQL | `db-f1-micro`, 10 GB SSD | ~$10 |
| Artifact Registry | < 1 GB | ~$0.10 |
| Secret Manager | < 10k accesses | ~$0.10 |
| **Total** | | **~$20/mo** |

> Use the [Google Cloud Pricing Calculator](https://cloud.google.com/products/calculator) for your specific usage.

---

## 3. DigitalOcean App Platform

Managed PaaS with automatic deploys from Docker images or a GitHub repository.

### Prerequisites

- A DigitalOcean account
- `doctl` CLI installed and authenticated (`doctl auth init`)
- A DigitalOcean Container Registry (or Docker Hub)

### Step 1 — Push image to DigitalOcean Container Registry

```bash
doctl registry create soroban-pulse-registry

docker tag soroban-pulse:latest registry.digitalocean.com/soroban-pulse-registry/soroban-pulse:latest
doctl registry login
docker push registry.digitalocean.com/soroban-pulse-registry/soroban-pulse:latest
```

### Step 2 — Create a managed PostgreSQL database

```bash
doctl databases create soroban-pulse-db \
  --engine pg \
  --version 16 \
  --size db-s-1vcpu-1gb \
  --region nyc1 \
  --num-nodes 1
```

Wait for the cluster to be ready, then retrieve the connection string:

```bash
doctl databases connection soroban-pulse-db --format URI
```

### Step 3 — Create the App Spec

Save as `do-app-spec.yaml`:

```yaml
name: soroban-pulse
region: nyc1

services:
  - name: soroban-pulse
    image:
      registry_type: DOCR
      registry: soroban-pulse-registry
      repository: soroban-pulse
      tag: latest
    http_port: 3000
    instance_count: 2
    instance_size_slug: professional-xs
    health_check:
      http_path: /healthz/ready
      initial_delay_seconds: 30
      period_seconds: 10
      failure_threshold: 3
    envs:
      - key: PORT
        value: "3000"
      - key: RUST_LOG
        value: info
      - key: RUST_LOG_FORMAT
        value: json
      - key: STELLAR_RPC_URL
        value: "https://soroban-testnet.stellar.org"
      - key: DB_MAX_CONNECTIONS
        value: "10"
      - key: RATE_LIMIT_PER_MINUTE
        value: "60"
      - key: START_LEDGER
        value: "0"
      - key: INDEXER_LOCK_RETRY_SECS
        value: "30"
      - key: DATABASE_URL
        value: "${soroban-pulse-db.DATABASE_URL}"
        type: SECRET
      - key: API_KEY
        value: "your-api-key"
        type: SECRET
      - key: ADMIN_API_KEY
        value: "your-admin-key"
        type: SECRET

databases:
  - name: soroban-pulse-db
    engine: PG
    version: "16"
    size: db-s-1vcpu-1gb
    num_nodes: 1
```

Deploy:

```bash
doctl apps create --spec do-app-spec.yaml
```

### Step 4 — Set up auto-deploy (optional)

Link a GitHub repository in the App Spec `source` block and DigitalOcean will
redeploy automatically on every push to `main`.

### DigitalOcean environment variables

```
DATABASE_URL          = (from managed DB)
STELLAR_RPC_URL       = https://soroban-testnet.stellar.org
PORT                  = 3000
RUST_LOG              = info
DB_MAX_CONNECTIONS    = 10
RATE_LIMIT_PER_MINUTE = 60
START_LEDGER          = 0
INDEXER_LOCK_RETRY_SECS = 30
```

### Cost estimate (nyc1, 2026)

| Resource | Spec | Est. monthly cost |
|---|---|---|
| App Platform (2 containers) | `professional-xs` (1 vCPU / 512 MB each) | ~$24 |
| Managed PostgreSQL | `db-s-1vcpu-1gb` | ~$15 |
| Container Registry | Starter plan (5 GB) | Free |
| **Total** | | **~$39/mo** |

> See [DigitalOcean pricing](https://www.digitalocean.com/pricing) for current rates.

---

## 4. Heroku

Container-based deployment on Heroku using Docker images.

### Prerequisites

- Heroku CLI installed (`brew install heroku`)
- Heroku account with a verified payment method
- Heroku Postgres add-on (or an external PostgreSQL URL)

### Step 1 — Log in and create an app

```bash
heroku login
heroku create soroban-pulse-app
```

### Step 2 — Configure the container stack

```bash
heroku stack:set container -a soroban-pulse-app
```

### Step 3 — Create `heroku.yml`

Save at the project root:

```yaml
build:
  docker:
    web: Dockerfile

run:
  web: ./soroban-pulse
```

### Step 4 — Add PostgreSQL

```bash
heroku addons:create heroku-postgresql:essential-0 -a soroban-pulse-app
```

Heroku automatically sets `DATABASE_URL`.  The existing `DATABASE_URL` variable
will be populated by the add-on.

### Step 5 — Set environment variables

```bash
APP=soroban-pulse-app

heroku config:set \
  PORT=3000 \
  STELLAR_RPC_URL="https://soroban-testnet.stellar.org" \
  RUST_LOG=info \
  RUST_LOG_FORMAT=json \
  DB_MAX_CONNECTIONS=5 \
  RATE_LIMIT_PER_MINUTE=60 \
  START_LEDGER=0 \
  INDEXER_LOCK_RETRY_SECS=30 \
  API_KEY="your-api-key" \
  ADMIN_API_KEY="your-admin-key" \
  -a "$APP"
```

### Step 6 — Deploy

```bash
git push heroku main
```

Heroku builds the Docker image and deploys automatically.

### Step 7 — Scale dynos

```bash
# 2 web dynos
heroku ps:scale web=2 -a "$APP"
```

### Step 8 — Enable health checks

Heroku uses the `CHECKS` file or HTTP health checks via the Dyno formation.
Add a `CHECKS` file at the project root:

```
WAIT=10
ATTEMPTS=6
/healthz/ready
```

### Heroku notes

- Heroku's free dynos **sleep after 30 minutes of inactivity** — use a paid
  plan for production.
- The SSE endpoint works on Heroku but connections are limited to **55 seconds**
  by the platform's request timeout.  Configure `SSE_KEEPALIVE_SECS=30` so
  proxies do not close idle SSE connections.
- Set `DB_MAX_CONNECTIONS=5` for the `essential-0` plan (25-connection limit).

### Heroku environment variables

```
DATABASE_URL          = (auto-set by Heroku Postgres add-on)
STELLAR_RPC_URL       = https://soroban-testnet.stellar.org
PORT                  = 3000
RUST_LOG              = info
DB_MAX_CONNECTIONS    = 5
RATE_LIMIT_PER_MINUTE = 60
START_LEDGER          = 0
INDEXER_LOCK_RETRY_SECS = 30
SSE_KEEPALIVE_SECS    = 30
```

### Cost estimate (2026)

| Resource | Spec | Est. monthly cost |
|---|---|---|
| Eco Dyno (1) | Shared, sleeps | ~$5 |
| Basic Dyno (1) | 512 MB, always on | ~$7 |
| Heroku Postgres | `essential-0` (1 GB) | ~$5 |
| **Total (basic)** | | **~$17/mo** |

> See [Heroku pricing](https://www.heroku.com/pricing) for current rates.

---

## 5. Railway

Simple, developer-friendly PaaS with native Docker support and managed
PostgreSQL.

### Prerequisites

- Railway account (railway.app)
- Railway CLI installed: `npm i -g @railway/cli`
- A GitHub repository connected to Railway (optional, for auto-deploy)

### Step 1 — Create a new project

```bash
railway login
railway init
```

Select **Empty Project** when prompted.

### Step 2 — Add a PostgreSQL database

In the Railway dashboard, click **+ New → Database → Add PostgreSQL**.

Railway automatically injects `DATABASE_URL` into your service.

### Step 3 — Deploy from Docker

Railway auto-detects your `Dockerfile`.  Connect your GitHub repository in the
**Settings → Source** panel, or deploy from CLI:

```bash
railway up
```

### Step 4 — Set environment variables

In the Railway dashboard go to your service → **Variables**, or via CLI:

```bash
railway variables set \
  PORT=3000 \
  STELLAR_RPC_URL="https://soroban-testnet.stellar.org" \
  RUST_LOG=info \
  RUST_LOG_FORMAT=json \
  DB_MAX_CONNECTIONS=10 \
  RATE_LIMIT_PER_MINUTE=60 \
  START_LEDGER=0 \
  INDEXER_LOCK_RETRY_SECS=30 \
  API_KEY="your-api-key" \
  ADMIN_API_KEY="your-admin-key"
```

Railway injects `DATABASE_URL` automatically from the linked PostgreSQL service —
do **not** set it manually.

### Step 5 — Custom domain (optional)

In **Settings → Networking**, generate a Railway subdomain or connect a custom
domain with automatic TLS.

### Step 6 — Scale replicas

In **Settings → Deploy**, increase replicas to 2 for high-availability.  The
advisory lock ensures only one indexes at a time.

### Railway environment variables

```
DATABASE_URL          = (auto-injected from Railway PostgreSQL)
STELLAR_RPC_URL       = https://soroban-testnet.stellar.org
PORT                  = 3000
RUST_LOG              = info
RUST_LOG_FORMAT       = json
DB_MAX_CONNECTIONS    = 10
RATE_LIMIT_PER_MINUTE = 60
START_LEDGER          = 0
INDEXER_LOCK_RETRY_SECS = 30
```

### Railway notes

- Railway's build cache speeds up subsequent deploys significantly.
- The free tier includes $5/month of usage — enough for light testing.
- SSE connections work without any special configuration.
- Auto-deploy from GitHub triggers on every push to the linked branch.

### Cost estimate (2026)

| Resource | Spec | Est. monthly cost |
|---|---|---|
| Service | ~512 MB RAM, shared CPU | ~$5–10 |
| PostgreSQL | Managed, 1 GB | ~$5 |
| **Total** | | **~$10–15/mo** |

> See [Railway pricing](https://railway.app/pricing) for current rates.

---

## Environment variable reference

These variables apply to all deployments.  See `.env.example` for the full list
and defaults.

| Variable | Required | Default | Notes |
|---|---|---|---|
| `DATABASE_URL` | Yes | — | PostgreSQL connection string |
| `STELLAR_RPC_URL` | Yes | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint |
| `PORT` | No | `3000` | HTTP port |
| `RUST_LOG` | No | `info` | Log level |
| `RUST_LOG_FORMAT` | No | `text` | Use `json` in production |
| `DB_MAX_CONNECTIONS` | No | `10` | Tune to your DB plan's connection limit |
| `DB_MIN_CONNECTIONS` | No | `1` | Keep-warm connections |
| `START_LEDGER` | No | `0` | `0` = start from latest ledger |
| `API_KEY` | No | — | Enables API key auth when set |
| `ADMIN_API_KEY` | No | — | Protects `/v1/admin/*` endpoints |
| `RATE_LIMIT_PER_MINUTE` | No | `60` | `0` = unlimited |
| `SSE_KEEPALIVE_SECS` | No | `15` | Increase to `30` on platforms with short timeouts |
| `INDEXER_LOCK_RETRY_SECS` | No | `30` | Standby replica retry interval |
| `HEALTH_CHECK_TIMEOUT_MS` | No | `2000` | DB ping timeout for health checks |
| `INDEXER_LAG_WARN_THRESHOLD` | No | `100` | Lag (ledgers) before warning |
| `SLOW_QUERY_THRESHOLD_MS` | No | `1000` | Queries slower than this are logged at WARN |

### Platform-specific recommendations

| Platform | DB_MAX_CONNECTIONS | SSE_KEEPALIVE_SECS | Notes |
|---|---|---|---|
| AWS ECS / RDS | 10–20 | 15 | RDS default max_connections ~87 on `db.t4g.micro` |
| Cloud Run / Cloud SQL | 10 | 15 | Cloud SQL proxy handles pooling |
| DigitalOcean / Managed PG | 10 | 15 | `db-s-1vcpu-1gb` allows 25 connections |
| Heroku Postgres | 5 | 30 | `essential-0` allows 25 connections; Heroku 55 s timeout |
| Railway Postgres | 10 | 15 | No hard timeout on connections |

---

## Cost summary

| Platform | Estimated monthly cost | Best for |
|---|---|---|
| AWS ECS (Fargate) + RDS | ~$70 | Production, high-traffic, AWS-native teams |
| Google Cloud Run + Cloud SQL | ~$20 | Cost-sensitive production, GCP-native teams |
| DigitalOcean App Platform | ~$39 | Simple PaaS, small teams |
| Heroku | ~$17 | Rapid prototyping, small deployments |
| Railway | ~$10–15 | Developer projects, hobby use |

> All estimates are approximate for a single-region, 2-replica deployment
> without dedicated caching.  Use the platform's own pricing calculator for
> accurate, region-specific numbers before deploying to production.
