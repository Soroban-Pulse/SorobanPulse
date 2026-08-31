// tests/load/ramp_up.js — Issue #923
//
// SCENARIO: Ramp-Up (Gradual Increase)
// ──────────────────────────────────────
// Purpose:  Verify that the service handles a gradually increasing user load
//           without degrading early.  Helps identify the load level at which
//           latency starts climbing or errors appear.
//
// Pattern:
//   Phase 1 — Ramp:  0 → 100 VUs over 5 minutes
//   Phase 2 — Hold:  100 VUs for 2 minutes (sustained peak)
//   Phase 3 — Ramp down: 100 → 0 VUs over 2 minutes
//
// SLO thresholds:
//   - p99 latency  < 300 ms  (relaxed from steady-state because VU count is higher)
//   - error rate   < 2 %
//
// Run:
//   k6 run tests/load/ramp_up.js
//   k6 run -e BASE_URL=http://localhost:3000 tests/load/ramp_up.js
//
// Env vars:
//   BASE_URL    Target service base URL (default: http://localhost:3000)
//   API_KEY     Optional API key

import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate, Counter } from "k6/metrics";

// ── Custom metrics ─────────────────────────────────────────────────────────
const latency    = new Trend("ru_latency_ms",  true);
const errorRate  = new Rate("ru_error_rate");
const throughput = new Counter("ru_requests_total");

// Track latency across each phase to detect when degradation starts
const rampLatency = new Trend("ru_ramp_phase_ms",  true);
const holdLatency = new Trend("ru_hold_phase_ms",  true);
const downLatency = new Trend("ru_down_phase_ms",  true);

// ── Configuration ──────────────────────────────────────────────────────────
const BASE_URL = (__ENV.BASE_URL || "http://localhost:3000").replace(/\/$/, "");
const HEADERS  = __ENV.API_KEY   ? { "X-Api-Key": __ENV.API_KEY } : {};

// ── Scenario ───────────────────────────────────────────────────────────────
export const options = {
  scenarios: {
    ramp_up: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: "5m", target: 100 },  // Ramp up to 100 VUs over 5 min
        { duration: "2m", target: 100 },  // Hold at 100 VUs for 2 min
        { duration: "2m", target: 0   },  // Ramp down over 2 min
      ],
      gracefulRampDown: "30s",
    },
  },
  thresholds: {
    ru_latency_ms: [
      // Overall — across all phases including the peak hold
      { threshold: "p(99)<300", abortOnFail: false },
      { threshold: "p(95)<200", abortOnFail: false },
    ],
    ru_error_rate: [
      { threshold: "rate<0.02", abortOnFail: true },
    ],
    // Phase-specific — useful for diagnosing where latency degrades
    ru_hold_phase_ms: [
      { threshold: "p(99)<300", abortOnFail: false },
    ],
  },
};

// ── Phase tracking ─────────────────────────────────────────────────────────
// Approximate phase boundaries based on start time.
// The ramp runs 0–5 min, hold 5–7 min, ramp-down 7–9 min.
const START_TS  = Date.now();

function currentPhase() {
  const elapsed = (Date.now() - START_TS) / 1000; // seconds
  if (elapsed < 300)  return "ramp";  // 0–5 min
  if (elapsed < 420)  return "hold";  // 5–7 min
  return "down";                       // 7–9 min
}

// ── Endpoint mix ───────────────────────────────────────────────────────────
// Mirrors the production traffic profile described in docs/sli-slo.md.
const ENDPOINT_MIX = [
  { weight: 50, path: () => `/v1/events?page=${rand(1,5)}&limit=20` },
  { weight: 25, path: () => `/v1/events?page=1&limit=20&event_type=contract` },
  { weight: 15, path: () => `/v1/events?from_ledger=${rand(1000000,1100000)}&to_ledger=${rand(1100001,1200000)}&limit=20` },
  { weight: 10, path: () => `/v1/events?exact_count=true&limit=20` },
];

function rand(min, max) {
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

function pickPath() {
  const total = ENDPOINT_MIX.reduce((s, e) => s + e.weight, 0);
  let r = Math.random() * total;
  for (const e of ENDPOINT_MIX) {
    r -= e.weight;
    if (r <= 0) return e.path();
  }
  return ENDPOINT_MIX[0].path();
}

// ── Main VU function ───────────────────────────────────────────────────────
export default function () {
  const path  = pickPath();
  const res   = http.get(`${BASE_URL}${path}`, { headers: HEADERS, timeout: "10s" });
  const phase = currentPhase();

  latency.add(res.timings.duration);
  throughput.add(1);

  // Record per-phase latency for analysis
  if (phase === "ramp") rampLatency.add(res.timings.duration);
  else if (phase === "hold") holdLatency.add(res.timings.duration);
  else downLatency.add(res.timings.duration);

  const ok = check(res, {
    "status 200":  (r) => r.status === 200,
    "has body":    (r) => r.body && r.body.length > 0,
  });
  errorRate.add(!ok);

  // Think time — slightly longer than constant_load to be realistic
  sleep(0.1 + Math.random() * 0.4);
}

// ── Summary ────────────────────────────────────────────────────────────────
export function handleSummary(data) {
  const m = data.metrics;

  function p(metric, stat) {
    const v = m[metric]?.values?.[stat];
    return v !== undefined ? `${Number(v).toFixed(1)} ms` : "n/a";
  }
  function pct(metric) {
    const v = m[metric]?.values?.rate;
    return v !== undefined ? `${(Number(v) * 100).toFixed(3)} %` : "n/a";
  }

  const p99  = m.ru_latency_ms?.values?.["p(99)"] ?? 0;
  const err  = m.ru_error_rate?.values?.rate       ?? 0;
  const pass = p99 < 300 && err < 0.02;

  console.log("\n=== RAMP-UP TEST SUMMARY ===");
  console.log(`Stages           : 0→100 VUs / 5 min ramp, 2 min hold, 2 min ramp-down`);
  console.log(`Total requests   : ${String(m.ru_requests_total?.values?.count ?? 0)}`);
  console.log(`\nOverall:`);
  console.log(`  p95            : ${p("ru_latency_ms", "p(95)")}`);
  console.log(`  p99            : ${p("ru_latency_ms", "p(99)")}  (SLO: < 300 ms)`);
  console.log(`  error rate     : ${pct("ru_error_rate")}  (SLO: < 2 %)`);
  console.log(`\nPer-phase p99:`);
  console.log(`  Ramp (0→100)   : ${p("ru_ramp_phase_ms", "p(99)")}`);
  console.log(`  Hold (100 VUs) : ${p("ru_hold_phase_ms", "p(99)")}`);
  console.log(`  Ramp-down      : ${p("ru_down_phase_ms", "p(99)")}`);
  console.log(`\nResult: ${pass ? "✅ PASS" : "❌ FAIL"}`);

  return {
    "tests/load/results/ramp_up_summary.json": JSON.stringify(data, null, 2),
  };
}
