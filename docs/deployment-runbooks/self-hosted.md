# Self-Hosted Deployment Runbook

Deploys SorobanPulse on infrastructure you already control — a bare-metal or
on-prem Linux server, or any VM you manage yourself — with **no cloud
provider assumed**. This runbook covers three sub-scenarios, from least to
most containerized:

- **(a) [Bare metal / on-prem, no containers](#a-bare-metal--on-prem-no-containers)**
  — the compiled `soroban-pulse` binary run directly under systemd.
- **(b) [Docker Compose](#b-docker-compose)** — the root
  [`docker-compose.yml`](../../docker-compose.yml) as-is, or lightly adapted
  for production.
- **(c) [systemd-managed Docker container](#c-systemd-managed-docker-container)**
  — a middle ground: still Docker, but supervised and auto-restarted by
  systemd instead of Docker's own `--restart` policy.

Pick (a) if you don't want a container runtime at all; (b) if you already run
Docker Compose elsewhere and want the fastest path; (c) if you want Docker's
packaging but systemd's process supervision, logging (`journalctl`), and
dependency ordering (e.g. starting after a local Postgres unit).

## Prerequisites

Common to all three sub-scenarios:

- A Linux server (physical or VM) with outbound HTTPS access to the Soroban
  RPC endpoint (`STELLAR_RPC_URL`, default
  `https://soroban-testnet.stellar.org`).
- A reachable PostgreSQL 16 instance — either on the same host, on another
  host you manage, or a managed database. SorobanPulse applies its own
  schema migrations automatically on startup (`db::run_migrations` in
  `src/main.rs`); no separate migration step is required before first boot.
- A copy of this repository (or at least `migrations/`, `Dockerfile`, and
  `docker-compose.yml`) checked out on a machine that can reach the target
  server.
- `sudo`/root access on the target server to install systemd units.
- (a) only: Rust toolchain matching `edition = "2021"` in
  [`Cargo.toml`](../../Cargo.toml) (or build on a separate machine and copy
  the binary over), plus `pkg-config` and `libssl-dev` (the same build
  dependencies the [`Dockerfile`](../../Dockerfile) installs).
- (b)/(c) only: Docker Engine with the Compose plugin (`docker compose
  version`).
- A reverse proxy (nginx/Caddy) if you need TLS — self-hosted deployments are
  not fronted by a cloud load balancer, so TLS termination is on you. See
  [docs/deployment.md § TLS Termination](../deployment.md#tls-termination) for
  full nginx/Caddy configuration; this runbook does not repeat it.

## Architecture

```
        (optional) nginx/Caddy — TLS termination, see docs/deployment.md
                                 │
                          HTTP :3000
                                 │
                 ┌───────────────────────────────┐
                 │  soroban-pulse                 │
                 │  (a) systemd + bare binary      │
                 │  (b) docker compose "app" svc   │
                 │  (c) systemd + docker container │
                 └───────────────┬───────────────┘
                                 │ 5432
                          PostgreSQL 16
                 (same host, another host, or managed)
```

All three sub-scenarios expose the same HTTP surface on `PORT` (default
`3000`) with `/healthz/ready` and `/healthz/live` health endpoints — the
difference is only in how the process is packaged and supervised. None of
them include a load balancer; if you need one, put nginx/Caddy in front (see
[docs/deployment.md](../deployment.md)) or adapt the ALB/Cloud LB/App Gateway
steps from the [aws.md](aws.md)/[gcp.md](gcp.md)/[azure.md](azure.md)
runbooks against your own network.

## Deployment Steps

### (a) Bare metal / on-prem, no containers

1. **Build the release binary.** From the repository root (or in CI, copying
   the artifact over afterward):
   ```bash
   cargo build --release
   # Binary name comes from Cargo.toml's [[bin]] section:
   #   name = "soroban-pulse", produced at target/release/soroban-pulse
   ```

2. **Install the binary and migrations.**
   ```bash
   sudo mkdir -p /opt/soroban-pulse/bin /opt/soroban-pulse/migrations
   sudo cp target/release/soroban-pulse /opt/soroban-pulse/bin/
   sudo cp -r migrations/. /opt/soroban-pulse/migrations/
   sudo useradd --system --no-create-home --shell /usr/sbin/nologin soroban || true
   sudo chown -R soroban:soroban /opt/soroban-pulse
   ```

3. **Write the environment file** (mirrors [`.env.example`](../../.env.example);
   keep it root-readable only since it holds `DATABASE_URL`/`API_KEY`):
   ```bash
   sudo install -m 640 -o root -g soroban /dev/null /etc/soroban-pulse.env
   sudo tee /etc/soroban-pulse.env >/dev/null <<'EOF'
   DATABASE_URL=postgres://soroban:CHANGE_ME@localhost:5432/soroban_pulse
   STELLAR_RPC_URL=https://soroban-testnet.stellar.org
   PORT=3000
   RUST_LOG=info
   RUST_LOG_FORMAT=json
   DB_MAX_CONNECTIONS=10
   RATE_LIMIT_PER_MINUTE=60
   START_LEDGER=0
   EOF
   ```

4. **Create the systemd unit** at `/etc/systemd/system/soroban-pulse.service`:
   ```ini
   [Unit]
   Description=SorobanPulse indexer/API
   After=network-online.target postgresql.service
   Wants=network-online.target

   [Service]
   Type=simple
   User=soroban
   Group=soroban
   WorkingDirectory=/opt/soroban-pulse
   EnvironmentFile=/etc/soroban-pulse.env
   ExecStart=/opt/soroban-pulse/bin/soroban-pulse
   Restart=on-failure
   RestartSec=5
   # Same idea as the Dockerfile's non-root soroban:soroban user
   NoNewPrivileges=true
   ProtectSystem=strict
   ReadWritePaths=/opt/soroban-pulse

   [Install]
   WantedBy=multi-user.target
   ```

5. **Enable and start it:**
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable --now soroban-pulse
   sudo systemctl status soroban-pulse
   ```

### (b) Docker Compose

1. **Use the root [`docker-compose.yml`](../../docker-compose.yml) as-is** for
   evaluation, or copy it to the target host and adapt for production:
   - Remove the `db` service's `ports: ["5432:5432"]` mapping if Postgres
     doesn't need to be reachable from outside the Docker network.
   - Set real values in a `.env` file next to `docker-compose.yml` (copy
     [`.env.example`](../../.env.example) as a starting point) rather than
     relying on the file's dev defaults (`soroban`/`soroban`).
   - Put a reverse proxy (nginx/Caddy) in front for TLS — see
     [docs/deployment.md § TLS Termination](../deployment.md#tls-termination).

2. **Bring the stack up** (same command the repo's `Makefile` uses):
   ```bash
   cp .env.example .env   # then edit .env with real credentials
   docker compose up --build -d
   docker compose ps
   ```
   Or via the Makefile target: `make docker-up`.

3. **Confirm the app service is healthy** (Compose already defines a
   healthcheck matching the Dockerfile's):
   ```bash
   docker compose ps app
   # STATUS column should read "healthy" once the /healthz/ready check passes
   ```

4. **Tear down / redeploy:**
   ```bash
   docker compose down          # stop, keep volumes (pgdata, promdata, ...)
   docker compose pull && docker compose up -d   # redeploy a new image tag
   ```

### (c) systemd-managed Docker container

Use this when you want Docker's image packaging but systemd's supervision
(auto-restart, `journalctl` logs, ordering against other units) instead of
`docker run --restart` or Compose.

1. **Build (or pull) the image** on the target host, or push to a registry
   and pull it there:
   ```bash
   docker build -t soroban-pulse:latest .
   ```

2. **Write the environment file** — same content as in scenario (a):
   ```bash
   sudo install -m 640 -o root -g root /dev/null /etc/soroban-pulse.env
   # ... populate as in step 3 of scenario (a), using the db container's
   # hostname/port instead of localhost if Postgres also runs in Docker
   ```

3. **Create the systemd unit** at
   `/etc/systemd/system/soroban-pulse-container.service`. Docker containers
   are foreground-run under `docker run --rm` so systemd can track the
   process; `ExecStartPre`/`ExecStop` clean up any stale container:
   ```ini
   [Unit]
   Description=SorobanPulse (Docker container, systemd-supervised)
   After=network-online.target docker.service
   Requires=docker.service
   Wants=network-online.target

   [Service]
   Type=simple
   TimeoutStartSec=0
   ExecStartPre=-/usr/bin/docker rm -f soroban-pulse
   ExecStart=/usr/bin/docker run --rm --name soroban-pulse \
     --env-file /etc/soroban-pulse.env \
     -p 3000:3000 \
     soroban-pulse:latest
   ExecStop=/usr/bin/docker stop -t 10 soroban-pulse
   Restart=on-failure
   RestartSec=5

   [Install]
   WantedBy=multi-user.target
   ```

4. **Enable and start it:**
   ```bash
   sudo systemctl daemon-reload
   sudo systemctl enable --now soroban-pulse-container
   sudo journalctl -u soroban-pulse-container -f
   ```

## Verification

All three sub-scenarios expose the same HTTP surface, so verification is
identical regardless of which you chose — substitute the right service name
where a command needs one:

```bash
# 1. Health checks (against the app's own port, or through your reverse proxy)
curl -sf http://localhost:3000/healthz/ready | jq .
curl -sf http://localhost:3000/healthz/live

# 2. Smoke-test a real API endpoint
curl -sf http://localhost:3000/v1/events?limit=1 | jq .

# 3. Database connectivity
psql "$DATABASE_URL" -c "SELECT 1;"

# 4. Indexer progress
curl -sf http://localhost:3000/metrics | grep soroban_pulse_indexer_current_ledger

# 5. Process/service status
# (a) bare metal:
sudo systemctl status soroban-pulse
# (b) Docker Compose:
docker compose ps app
# (c) systemd-managed container:
sudo systemctl status soroban-pulse-container
```

## Rollback

- **(a) Bare metal**: keep the previous binary alongside the new one
  (e.g. `soroban-pulse.previous`) before overwriting it, so you can restore
  it and restart:
  ```bash
  sudo systemctl stop soroban-pulse
  sudo cp /opt/soroban-pulse/bin/soroban-pulse.previous /opt/soroban-pulse/bin/soroban-pulse
  sudo systemctl start soroban-pulse
  ```
- **(b) Docker Compose**: pin the `app` service to the previous image tag
  and redeploy: `docker compose up -d app` after editing the `image:`/`build:`
  reference, or `docker compose down && git checkout <previous-commit> &&
  docker compose up --build -d`.
- **(c) systemd-managed container**: retag or re-pull the previous image
  under the same local tag, then restart the unit:
  ```bash
  docker tag soroban-pulse:<previous-tag> soroban-pulse:latest
  sudo systemctl restart soroban-pulse-container
  ```
- **All three**: if a database migration shipped with the bad release, follow
  the migration rollback procedure in
  [docs/deployment.md § Migration Strategy](../deployment.md#migration-strategy)
  before rolling the app back, so the old binary/image isn't pointed at a
  newer schema than it understands.

## Troubleshooting

### (a) Bare metal / on-prem

| Symptom | Likely cause | Fix |
|---|---|---|
| `systemctl status` shows `activating (auto-restart)` in a loop | Binary panics on startup — often a missing/invalid env var | `journalctl -u soroban-pulse -n 100 --no-pager`; verify `/etc/soroban-pulse.env` against [`.env.example`](../../.env.example) |
| `Permission denied` starting the binary | `soroban` user lacks execute permission, or `ProtectSystem=strict` blocks a path the app needs | `ls -l /opt/soroban-pulse/bin/soroban-pulse`; add the path to `ReadWritePaths=` in the unit if genuinely needed |
| App starts but can't reach Postgres | `postgresql.service` not actually up yet despite `After=` ordering, or wrong `DATABASE_URL` host | `systemctl status postgresql`; `psql "$DATABASE_URL" -c "SELECT 1;"` as the `soroban` user |
| Binary fails to build (`error: linking with cc failed`, missing `libssl`) | Missing build dependencies the Dockerfile installs (`pkg-config`, `libssl-dev`) | `sudo apt-get install -y pkg-config libssl-dev` (matches the `builder` stage in the [Dockerfile](../../Dockerfile)) |

### (b) Docker Compose

| Symptom | Likely cause | Fix |
|---|---|---|
| `docker compose ps` shows `app` as `unhealthy` | DB not ready when app started, or bad env var | `docker compose logs app --tail 100`; confirm `depends_on: db: condition: service_healthy` is intact (it is in the stock `docker-compose.yml`) |
| `db` service fails `pg_isready` healthcheck | Volume permissions issue after a host migration, or leftover data from an incompatible Postgres version | `docker compose logs db --tail 100`; as a last resort for a fresh environment, `docker compose down -v` to drop the `pgdata` volume (destroys data — do not do this on a real deployment) |
| Port `3000`/`5432` already in use on the host | Another process bound the port | `ss -ltnp | grep -E ':3000|:5432'`; change the host-side mapping in `docker-compose.yml` (e.g. `"3001:3000"`) |
| Changes to `.env` don't take effect | Compose only re-reads `.env` on `up`, not on a running container | `docker compose up -d` again after editing `.env` (add `--force-recreate` if the container doesn't pick it up) |

### (c) systemd-managed Docker container

| Symptom | Likely cause | Fix |
|---|---|---|
| `systemctl start` fails immediately with `docker: command not found` equivalent | Docker Engine not installed or `docker.service` not running | `systemctl status docker`; `sudo systemctl enable --now docker` |
| Old container still running after a redeploy | `ExecStartPre=-/usr/bin/docker rm -f soroban-pulse` didn't run (unit not restarted) | `sudo systemctl restart soroban-pulse-container` (not just `start`) so `ExecStartPre` fires |
| `journalctl -u soroban-pulse-container` shows nothing | Container logs go to Docker's own log driver, not directly to the journal, when run detached — but this unit runs it in the foreground (`docker run` without `-d`), so logs should appear; if not, check the container actually started | `docker ps -a | grep soroban-pulse`; `docker logs soroban-pulse` |
| systemd reports the unit `exited, code=exited, status=125` | `docker run` itself failed (bad flag, image not found) before the app even started | Run the same `ExecStart` command manually to see Docker's own error output |

For anything beyond the deployment itself (indexer lag, RPC errors, webhook
failures, connection pool exhaustion once the service is live), use
[docs/runbooks/operator-runbook.md](../runbooks/operator-runbook.md) and
[docs/runbooks/db-pool-exhaustion.md](../runbooks/db-pool-exhaustion.md).
