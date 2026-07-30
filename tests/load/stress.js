// k6 stress test for Soroban Pulse — issue #811
//
// Progressively ramps load from 2× baseline (200 req/s) up to 10× (1 000 req/s)
// to find the breaking point and measure degradation behaviour.
//
// Run:  k6 run tests/load/stress.js
// Env:  BASE_URL   (default http://localhost:3000)
//       API_KEY    (optional, sent as X-Api-Key when set)

import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate, Counter } from "k6/metrics";

// ── Custom metrics ─────────────────────────────────────────────────────────
const latency       = new Trend("stress_latency_ms", true);
const errorRate     = new Rate("stress_error_rate");
const throughput    = new Counter("stress_requests_total");
const p99Breach     = new Counter("stress_p99_slo_breaches");

// ── Scenario configuration ─────────────────────────────────────────────────
// Stages climb from 2× → 4× → 6× → 8× → 10× baseline then hold and recover.
export const options = {
  scenarios: {
    stress_ramp: {
      executor: "ramping-arrival-rate",
      startRate: 200,           // 2× baseline (100 req/s)
      timeUnit: "1s",
      stages: [
        { duration: "2m",  target: 200  },  // Warm-up at 2×
        { duration: "3m",  target: 400  },  // 4×
        { duration: "3m",  target: 600  },  // 6×
        { duration: "3m",  target: 800  },  // 8×
        { duration: "3m",  target: 1000 },  // 10× — stress peak
        { duration: "2m",  target: 1000 },  // Hold peak
        { duration: "2m",  target: 100  },  // Recovery — back to baseline
      ],
      preAllocatedVUs: 200,
      maxVUs: 500,
    },
  },
  thresholds: {
    // SLOs degrade gracefully under stress — we track but don't hard-fail.
    stress_latency_ms: [
      { threshold: "p(95)<500",  abortOnFail: false },
      { threshold: "p(99)<2000", abortOnFail: false },
    ],
    stress_error_rate: [
      { threshold: "rate<0.05", abortOnFail: false },
    ],
  },
};

const BASE_URL = __ENV.BASE_URL || "http://localhost:3000";
const HEADERS  = __ENV.API_KEY
  ? { "X-Api-Key": __ENV.API_KEY }
  : {};

// Endpoint mix: weighted towards the most common read path.
const ENDPOINTS = [
  { weight: 60, path: "/v1/events?page=1&limit=20" },
  { weight: 20, path: "/v1/events?page=1&limit=20&event_type=contract" },
  { weight: 10, path: "/v1/events?from_ledger=1000000&to_ledger=1100000&limit=20" },
  { weight:  5, path: "/v1/events?exact_count=true&limit=20" },
  { weight:  5, path: "/healthz/ready" },
];

function weightedPick() {
  const total = ENDPOINTS.reduce((s, e) => s + e.weight, 0);
  let r = Math.random() * total;
  for (const e of ENDPOINTS) {
    r -= e.weight;
    if (r <= 0) return e.path;
  }
  return ENDPOINTS[0].path;
}

export default function () {
  const path = weightedPick();
  const res  = http.get(`${BASE_URL}${path}`, { headers: HEADERS, timeout: "10s" });

  latency.add(res.timings.duration);
  throughput.add(1);

  const ok = check(res, {
    "status 200": (r) => r.status === 200,
    "body not empty": (r) => r.body && r.body.length > 0,
  });
  errorRate.add(!ok);

  if (res.timings.duration > 200) {
    p99Breach.add(1);
  }
}

export function handleSummary(data) {
  const p99 = data.metrics.stress_latency_ms?.values?.["p(99)"] ?? "n/a";
  const p95 = data.metrics.stress_latency_ms?.values?.["p(95)"] ?? "n/a";
  const err = data.metrics.stress_error_rate?.values?.rate ?? "n/a";
  const rps = data.metrics.stress_requests_total?.values?.count ?? 0;

  console.log("\n=== STRESS TEST SUMMARY ===");
  console.log(`Total requests : ${rps}`);
  console.log(`p95 latency    : ${typeof p95 === "number" ? p95.toFixed(1) : p95} ms`);
  console.log(`p99 latency    : ${typeof p99 === "number" ? p99.toFixed(1) : p99} ms`);
  console.log(`Error rate     : ${typeof err === "number" ? (err * 100).toFixed(2) : err} %`);

  return {
    "tests/load/results/stress_summary.json": JSON.stringify(data, null, 2),
  };
}
