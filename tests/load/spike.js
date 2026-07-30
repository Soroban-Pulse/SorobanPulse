// k6 spike test for Soroban Pulse — issue #811
//
// Simulates a sudden 10× traffic burst (1 000 req/s) from a stable 100 req/s
// baseline, then returns to baseline.  Tests elasticity and recovery speed.
//
// Run:  k6 run tests/load/spike.js
// Env:  BASE_URL   (default http://localhost:3000)
//       API_KEY    (optional)

import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate, Counter, Gauge } from "k6/metrics";

// ── Custom metrics ─────────────────────────────────────────────────────────
const latency          = new Trend("spike_latency_ms", true);
const errorRate        = new Rate("spike_error_rate");
const throughput       = new Counter("spike_requests_total");
const recoveryLatency  = new Trend("spike_recovery_latency_ms", true);

// ── Scenario ───────────────────────────────────────────────────────────────
export const options = {
  scenarios: {
    // Baseline before the spike
    pre_spike_baseline: {
      executor: "constant-arrival-rate",
      rate: 100,
      timeUnit: "1s",
      duration: "1m",
      preAllocatedVUs: 20,
      maxVUs: 50,
      exec: "baseline",
      startTime: "0s",
    },
    // Instant 10× spike for 30 seconds
    spike_burst: {
      executor: "constant-arrival-rate",
      rate: 1000,
      timeUnit: "1s",
      duration: "30s",
      preAllocatedVUs: 200,
      maxVUs: 600,
      exec: "spike",
      startTime: "1m",       // starts right after pre-spike
    },
    // Return to baseline and measure recovery
    post_spike_recovery: {
      executor: "constant-arrival-rate",
      rate: 100,
      timeUnit: "1s",
      duration: "2m",
      preAllocatedVUs: 20,
      maxVUs: 50,
      exec: "recovery",
      startTime: "1m30s",    // starts as spike is ending
    },
  },
  thresholds: {
    spike_latency_ms: [
      // During spike we allow up to 5 s p99
      { threshold: "p(99)<5000", abortOnFail: false },
    ],
    spike_error_rate: [
      // Allow up to 10 % errors during the burst itself
      { threshold: "rate<0.10", abortOnFail: false },
    ],
    spike_recovery_latency_ms: [
      // After burst, p99 must return to normal SLO within the recovery window
      { threshold: "p(99)<200", abortOnFail: false },
    ],
  },
};

const BASE_URL = __ENV.BASE_URL || "http://localhost:3000";
const HEADERS  = __ENV.API_KEY ? { "X-Api-Key": __ENV.API_KEY } : {};

function request(path) {
  return http.get(`${BASE_URL}${path}`, { headers: HEADERS, timeout: "15s" });
}

// Pre-spike: normal baseline traffic
export function baseline() {
  const res = request("/v1/events?page=1&limit=20");
  latency.add(res.timings.duration);
  throughput.add(1);
  const ok = check(res, { "status 200": (r) => r.status === 200 });
  errorRate.add(!ok);
}

// The spike itself — hammers a mix of endpoints simultaneously
export function spike() {
  const paths = [
    "/v1/events?page=1&limit=20",
    "/v1/events?page=1&limit=20&event_type=contract",
    "/v1/events?from_ledger=1000000&limit=20",
    "/healthz/ready",
  ];
  const path = paths[Math.floor(Math.random() * paths.length)];
  const res  = request(path);

  latency.add(res.timings.duration);
  throughput.add(1);
  const ok = check(res, { "status 200 or 429": (r) => r.status === 200 || r.status === 429 });
  errorRate.add(!ok);
}

// Recovery: baseline traffic, measuring how quickly p99 returns to SLO
export function recovery() {
  const res = request("/v1/events?page=1&limit=20");
  recoveryLatency.add(res.timings.duration);
  throughput.add(1);
  const ok = check(res, { "status 200": (r) => r.status === 200 });
  errorRate.add(!ok);
}

export function handleSummary(data) {
  const spikep99    = data.metrics.spike_latency_ms?.values?.["p(99)"]          ?? "n/a";
  const recovp99    = data.metrics.spike_recovery_latency_ms?.values?.["p(99)"] ?? "n/a";
  const err         = data.metrics.spike_error_rate?.values?.rate                ?? "n/a";
  const total       = data.metrics.spike_requests_total?.values?.count           ?? 0;

  console.log("\n=== SPIKE TEST SUMMARY ===");
  console.log(`Total requests     : ${total}`);
  console.log(`Spike p99 latency  : ${typeof spikep99 === "number" ? spikep99.toFixed(1) : spikep99} ms`);
  console.log(`Recovery p99       : ${typeof recovp99 === "number" ? recovp99.toFixed(1) : recovp99} ms`);
  console.log(`Error rate         : ${typeof err === "number" ? (err * 100).toFixed(2) : err} %`);

  return {
    "tests/load/results/spike_summary.json": JSON.stringify(data, null, 2),
  };
}
