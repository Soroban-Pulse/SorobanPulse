# Load Testing Guide

> **Issue #923** — Comprehensive load testing scenario suite for SorobanPulse.
>
> This guide covers prerequisites, scenario descriptions, how to run tests locally
> and in CI, how to interpret k6 output, and how to extend the suite.

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [SLO reference](#slo-reference)
3. [Scenarios](#scenarios)
   - [Constant Load](#1-constant-load)
   - [Ramp-Up](#2-ramp-up)
   - [Burst](#3-burst)
   - [Sustained Overload](#4-sustained-overload)
   - [Stress Test](#5-stress-test)
   - [Soak Test](#6-soak-test)
   - [Spike Test](#7-spike-test)
4. [Running locally](#running-locally)
5. [Running in CI](#running-in-ci)
6. [Reading k6 output](#reading-k6-output)
7. [Extending the suite](#extending-the-suite)
8. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### 1. Install k6

```bash
# macOS
brew install k6

# Linux (Debian/Ubuntu)
sudo gpg -k
sudo gpg --no-default-keyring \
  --keyring /usr/share/keyrings/k6-archive-keyring.gpg \
  --keyserver hkp://keyserver.ubuntu.com:80 \
  --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" \
  | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update && sudo apt-get install -y k6

# Docker alternative (no install required)
docker run --rm -i grafana/k6 run - < tests/load/constant_load.js
```

### 2. Start the service and database

```bash
# Full stack with Docker Compose
make docker-up

# Or run locally
make run   # starts the service; ensure DATABASE_URL is set in .env
```

### 3. Verify the service is healthy

```bash
curl http://localhost:3000/healthz/ready
# Expected: {"status":"ok","db":"ok","indexer":"ok"}
```

### 4. (Optional) Seed test data

Load tests are most representative when the database contains realistic event data.

```bash
# Seed with the integration test dataset
PGPASSWORD=postgres psql -h localhost -U postgres soroban_pulse_test \
  -f tests/e2e/seed.sql
```

---

## SLO reference

These targets come from `docs/sli-slo.md` and `README.md`.

| Metric | Target | Notes |
|---|---|---|
| `GET /v1/events` p99 latency | < 200 ms | At 100 req/s steady-state |
| Error rate | < 1 % | All endpoints combined |
| SSE connection establishment p99 | < 500 ms | |
| SSE time-to-first-byte p99 | < 1 s | |

Scenarios either enforce these thresholds as hard pass/fail gates (constant load,
ramp-up, burst recovery) or use relaxed/observational thresholds where degradation
is expected by design (overload, stress, soak).

---

## Scenarios

### 1. Constant Load

**File:** `tests/load/constant_load.js`  
**Make target:** `make load-test-constant`

The baseline steady-state test. 50 virtual users continuously send a realistic
mix of requests for 5 minutes. This is the scenario that should always pass on
every commit — it validates the SLO at typical production load.

**When to run:** Before every significant change to handlers, database queries,
or connection pool configuration.

**Thresholds:**
| Threshold | Value |
|---|---|
| p99 latency | < 200 ms |
| p95 latency | < 150 ms |
| Error rate  | < 1 %    |

**Endpoint mix:**
| Endpoint | Weight |
|---|---|
| `GET /v1/events?page=N&limit=20` | 50 % |
| `GET /v1/events/{contract_id}` | 30 % |
| `GET /v1/events/tx/{hash}` | 20 % |

---

### 2. Ramp-Up

**File:** `tests/load/ramp_up.js`  
**Make target:** `make load-test-ramp`

Gradually increases concurrent users from 0 to 100 over 5 minutes, holds for
2 minutes at peak, then ramps back down over 2 minutes. Useful for finding the
load level at which latency starts to climb and for validating that the connection
pool scales correctly.

**When to run:** After changing `DB_MAX_CONNECTIONS`, the connection pool settings,
or the Axum worker configuration.

**Stages:**
| Stage | Duration | VUs |
|---|---|---|
| Ramp-up | 5 min | 0 → 100 |
| Hold | 2 min | 100 |
| Ramp-down | 2 min | 100 → 0 |

**Thresholds:**
| Threshold | Value |
|---|---|
| Overall p99 | < 300 ms |
| Hold-phase p99 | < 300 ms |
| Error rate | < 2 % |

The summary output breaks down p99 latency for each phase, so you can see exactly
where degradation starts.

---

### 3. Burst

**File:** `tests/load/burst.js`  
**Make target:** `make load-test-burst`

Simulates two sudden traffic spikes on top of a continuous baseline load. Models
events like a contract being referenced in a trending blog post, or a wave of
clients reconnecting after a brief network interruption.

**When to run:** After changes to rate limiting, connection pooling, or the
advisory-lock indexer coordination.

**Pattern:**
- 10 VU background traffic throughout (4 min)
- Two spikes: 200 req/s for 30 s each (via `ramping-arrival-rate`)
- Cooldown between spikes: 60 s

**Thresholds:**
| Threshold | Value | Note |
|---|---|---|
| Spike p99 | < 500 ms | Relaxed during the burst |
| Cooldown p99 | < 200 ms | Must recover to normal SLO |
| Error rate | < 5 % | 429 rate-limit responses are counted as successes |

A test that passes on spike p99 but fails on cooldown p99 indicates the service
does not release resources quickly enough after a burst.

---

### 4. Sustained Overload

**File:** `tests/load/sustained_overload.js`  
**Make target:** `make load-test-overload`

Drives the service at 200 req/s (2× the SLO baseline) for 3 minutes and records
how latency and error rate behave. This scenario does **not** fail CI — it is
purely observational.

**When to run:** When you want to know the service's actual capacity headroom,
or when preparing for a traffic event (launch, promotion, etc.).

**Phases:**
| Phase | Rate | Duration |
|---|---|---|
| Warm-up | 100 req/s | 30 s |
| Overload | 200 req/s (default) | 3 min |
| Recovery | 100 req/s | 1 min |

**What to look for in the summary:**
- Does p99 exceed 200 ms during overload?
- Does the error rate exceed 1 % during overload?
- Does the service recover to < 200 ms p99 after the overload ends?

Override the overload rate with `OVERLOAD_RATE=300 make load-test-overload`.

---

### 5. Stress Test

**File:** `tests/load/stress.js`  
**Make target:** `make load-test-stress`

Existing file (Issue #811). Progressively ramps from 200 req/s to 1000 req/s
to find the breaking point. Observational thresholds only.

**When to run:** Quarterly capacity reviews, or before a major infrastructure change.

---

### 6. Soak Test

**File:** `tests/load/soak.js`  
**Make target:** `make load-test-soak`

Existing file (Issue #811). Sustained 24-hour run at baseline load to detect
memory leaks and gradual performance degradation. Use `SOAK_DURATION=30m` for
a quick validation during development.

**When to run:** Before major releases, or when investigating suspected memory leaks.

```bash
SOAK_DURATION=30m make load-test-soak
```

---

### 7. Spike Test

**File:** `tests/load/spike.js`  
**Make target:** `make load-test-spike`

Existing file (Issue #811). Instant 10× traffic burst (1000 req/s) from a 100
req/s baseline, then returns to baseline. Tests elasticity and recovery speed.

---

## Running locally

### Quick smoke check (60 seconds)

```bash
make load-test-quick
```

Runs the constant-load scenario for 60 seconds with 50 VUs. Fast way to confirm
the service is behaving before running the full suite.

### Individual scenarios

```bash
make load-test-constant    # 50 VUs, 5 min
make load-test-ramp        # 0→100 VUs ramp
make load-test-burst       # two spikes
make load-test-overload    # 2× overload, observational
make load-test-stress      # breaking point
make load-test-soak        # long-duration (use SOAK_DURATION=30m locally)
make load-test-spike       # 10× instant spike
```

### Full suite (all scenarios)

```bash
make load-test
```

Runs scenarios 1–6 sequentially and prints a pass/fail summary. Skips the soak
test by default; add it manually if needed.

### Pointing at a non-default host

```bash
BASE_URL=https://staging.example.com make load-test-constant
# or with k6 directly:
k6 run -e BASE_URL=https://staging.example.com tests/load/constant_load.js
```

### With authentication

```bash
API_KEY=my-secret k6 run tests/load/constant_load.js
# or
K6_FLAGS="-e API_KEY=my-secret" make load-test-quick
```

### Saving results

All `handleSummary` functions write JSON to `tests/load/results/`. Results are
gitignored (see `.gitignore`).

```bash
# After running:
ls tests/load/results/
# constant_load_summary.json  ramp_up_summary.json  burst_summary.json ...
```

---

## Running in CI

### Workflow: `.github/workflows/load-tests.yml`

Load tests run automatically on:
- Push to `main` (when `src/`, `migrations/`, or `tests/load/` change)
- Weekly schedule (Monday 03:00 UTC)
- Manual `workflow_dispatch` (choose specific scenario)

The CI pipeline:
1. Builds a release binary from source.
2. Starts a PostgreSQL service container and runs migrations.
3. Seeds the database and starts the service.
4. Runs each scenario in a separate job.
5. Checks results against the regression baseline in `tests/load/baselines.json`.
6. Posts a summary comment on PRs.
7. Uploads raw results as artifacts (retained 30 days).

### Triggering manually

Go to **Actions → Load Tests → Run workflow** and optionally:
- Set `scenario` to run a single test (e.g. `constant_load`)
- Set `db_size` to `100k`, `1M`, or `10M`
- Set `soak_duration` to override the soak test length

---

## Reading k6 output

### Terminal summary

k6 prints a summary after each run. Key fields:

```
scenarios: (100.00%) 1 scenario, 50 max VUs, 5m30s max duration
default: 50 looping VUs for 5m0s (gracefulStop: 30s)

✓ status 200
✓ has body
✓ valid JSON

checks.........................: 99.92% ✓ 14987  ✗ 12
data_received..................: 45 MB  150 kB/s
data_sent......................: 1.2 MB 4.0 kB/s
http_req_duration..............: avg=12.3ms min=1.1ms med=9.2ms
                                 max=198.6ms p(90)=28.4ms p(95)=45.2ms p(99)=112.7ms
vus............................: 50     min=50       max=50
```

**Key metrics:**
- `p(99)` — the threshold that maps to the SLO
- `checks` — pass rate for your `check()` assertions
- `http_req_duration` — raw request timing breakdown

### Custom scenario summaries

Each script in this suite prints its own structured summary via `handleSummary`:

```
=== CONSTANT LOAD TEST SUMMARY ===
Duration         : 5m
Total requests   : 15000
p95 latency      : 45.2 ms
p99 latency      : 112.7 ms  (SLO: < 200 ms)
Error rate       : 0.080 %  (SLO: < 1%)

Endpoint breakdown:
  GET /v1/events              p99: 108.3 ms
  GET /v1/events/{id}         p99: 121.4 ms
  GET /v1/events/tx/{hash}    p99: 98.2 ms

Result: ✅ PASS
```

### Threshold pass/fail

k6 exits with code `0` if all thresholds pass, non-zero if any fail. The `make`
targets propagate this exit code, so `make load-test` prints ✅/❌ per scenario.

### JSON output

```bash
k6 run --out json=tests/load/results/raw.json tests/load/constant_load.js
```

The raw JSON contains per-iteration data points and can be analysed with `jq`:

```bash
jq '.metrics.cl_latency_ms.values["p(99)"]' tests/load/results/constant_load_summary.json
```

---

## Extending the suite

### Adding a new scenario

1. Create `tests/load/<scenario_name>.js` following the pattern of existing scripts:
   - Top-of-file comment block explaining purpose, pattern, SLOs
   - `BASE_URL` from `__ENV.BASE_URL`
   - Custom `Trend`/`Rate`/`Counter` metrics (prefixed to avoid collisions)
   - `export const options` with `scenarios` and `thresholds`
   - `handleSummary` for structured console output and JSON result file

2. Add a make target to `Makefile`:
   ```makefile
   load-test-<name>: ## <description>
       @command -v k6 >/dev/null 2>&1 || { echo "k6 not installed."; exit 1; }
       @mkdir -p tests/load/results
       k6 run ${K6_FLAGS} tests/load/<scenario_name>.js
   ```

3. Add the scenario to `make load-test` in the sequential list.

4. If the scenario has hard SLO thresholds, add it to the CI workflow in
   `.github/workflows/load-tests.yml`.

5. Document it in this guide under [Scenarios](#scenarios).

### Adding authentication

When `API_KEY` is set, all scripts automatically include it as `X-Api-Key`. For
scenarios that test admin endpoints, set `ADMIN_API_KEY` instead.

### Custom contract IDs

Set the `CONTRACT_IDS` environment variable as a comma-separated list:

```bash
k6 run -e CONTRACT_IDS="CABC...,CDEF..." tests/load/constant_load.js
```

Scripts that use the `lib/helpers.js` `resolveContractIds()` function pick this
up automatically.

---

## Troubleshooting

### k6 reports too many "dropped iterations"

The service cannot handle the requested arrival rate. Either:
- Reduce the `rate` in the scenario
- Increase `preAllocatedVUs` / `maxVUs`
- Check that the service has enough database connections (`DB_MAX_CONNECTIONS`)

### All requests return 429

The rate limiter is rejecting requests. Either:
- Set `RATE_LIMIT_PER_MINUTE=0` in your test environment to disable rate limiting
- Or reduce the request rate so it stays below the limit (default: 60 req/min per IP)

For load testing you generally want to test without rate limiting unless you are
specifically testing the rate limiter itself.

### p99 is much higher than expected

Check:
1. Is the database properly seeded? Empty tables return instantly; large tables are more realistic.
2. Is the service using a release binary? `make load-test-quick` uses whatever is at `PORT=3000`.
3. Are there other processes competing for CPU on the test machine?
4. Is `RUST_LOG` set to `debug` or `trace`? Set `RUST_LOG=warn` for load tests.

### k6 runs but threshold output says "n/a"

The metric name in `handleSummary` does not match the `Trend`/`Rate` variable name.
Check that the metric name string passed to `new Trend("name", ...)` matches the
key used in `data.metrics["name"]`.

### Service crashes during load test

Check:
- `docker logs soroban-pulse` or the process stdout for panic messages
- Database connection limit: `SHOW max_connections;` in psql
- OS file descriptor limit: `ulimit -n` (should be ≥ 65536 for load testing)

### CI load tests time out

GitHub-hosted runners have limited CPU. If tests time out:
- Reduce scenario duration via environment variables
- Move long-running scenarios (soak, stress) to the weekly schedule only
- Use `workflow_dispatch` with a reduced duration for on-demand runs

---

## See also

- `tests/load/lib/helpers.js` — shared k6 utilities (URL builders, weighted pickers, SLO constants)
- `tests/load/baselines.json` — regression baseline values used by CI
- `tests/load/regression_check.js` — baseline comparison script
- `docs/sli-slo.md` — full SLO definitions
- `docs/load-testing-runbook.md` — operational runbook for load test incidents
- [k6 documentation](https://k6.io/docs/)
