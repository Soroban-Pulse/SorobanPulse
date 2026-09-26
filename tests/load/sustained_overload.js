// tests/load/sustained_overload.js — Issue #923
//
// SCENARIO: Sustained Overload (Observational)
// ──────────────────────────────────────────────
// Purpose:  Drive the service at 2× its target SLO rate (200 req/s vs the
//           100 req/s SLO baseline) for 3 minutes and observe at what point
//           latency and error rate cross SLO thresholds.
//
//           This is an OBSERVATIONAL test — thresholds are informational only
//           and will NOT cause CI to fail.  The goal is to characterise service
//           behaviour under overload, not to gate deployments.
//
// Pattern:
//   Warm-up:  100 req/s for 30 s
//   Overload: 200 req/s for 3 min
//   Recovery: 100 req/s for 1 min
//
// Metrics to watch:
//   - soload_latency_ms  p99 — when does it exceed 200 ms?
//   - soload_error_rate  — when does it exceed 1 %?
//   - soload_requests_total — actual throughput delivered
//
// Run:
//   k6 run tests/load/sustained_overload.js
//   k6 run -e BASE_URL=http://localhost:3000 tests/load/sustained_overload.js
//
// Env vars:
//   BASE_URL           Target service base URL (default: http://localhost:3000)
//   API_KEY            Optional API key
//   OVERLOAD_RATE      Override the overload arrival rate (default: 200)
//   OVERLOAD_DURATION  Override overload phase duration   (default: 3m)

import http from "k6/http";
import { check } from "k6";
import { Trend, Rate, Counter } from "k6/metrics";

// ── Custom metrics ─────────────────────────────────────────────────────────
const latency    = new Trend("soload_latency_ms",  true);
const errorRate  = new Rate("soload_error_rate");
const throughput = new Counter("soload_requests_total");

// Phase-level metrics allow pinpointing where degradation starts
const warmupLatency    = new Trend("soload_warmup_ms",    true);
const overloadLatency  = new Trend("soload_overload_ms",  true);
const recoveryLatency  = new Trend("soload_recovery_ms",  true);

// ── Configuration ──────────────────────────────────────────────────────────
const BASE_URL         = (__ENV.BASE_URL         || "http://localhost:3000").replace(/\/$/, "");
const HEADERS          = __ENV.API_KEY           ? { "X-Api-Key": __ENV.API_KEY } : {};
const OVERLOAD_RATE    = parseInt(__ENV.OVERLOAD_RATE    || "200", 10);
const OVERLOAD_DURATION = __ENV.OVERLOAD_DURATION || "3m";

// ── Scenario ───────────────────────────────────────────────────────────────
export const options = {
  scenarios: {
    sustained_overload: {
      executor: "ramping-arrival-rate",
      startRate: 100,
      timeUnit: "1s",
      stages: [
        // Warm-up at baseline rate
        { duration: "30s",            target: 100            },
        // Ramp to overload
        { duration: "15s",            target: OVERLOAD_RATE  },
        // Sustain the overload
        { duration: OVERLOAD_DURATION, target: OVERLOAD_RATE },
        // Recovery
        { duration: "15s",            target: 100            },
        { duration: "1m",             target: 100            },
      ],
      preAllocatedVUs: 100,
      maxVUs: 400,
    },
  },

  // ── INTENTIONALLY PERMISSIVE thresholds ───────────────────────────────
  // This scenario is observational.  Thresholds are set to values we expect
  // the service to exceed so that the test itself never blocks a deployment.
  // Review the summary output manually to understand degradation behaviour.
  thresholds: {
    soload_latency_ms: [
      { threshold: "p(99)<5000", abortOnFail: false },  // abort only on catastrophic failure
    ],
    soload_error_rate: [
      { threshold: "rate<0.50",  abortOnFail: false },  // abort only at 50% error rate
    ],
  },
};

// ── Phase tracking ─────────────────────────────────────────────────────────
const START_TS = Date.now();

function currentPhase() {
  const s = (Date.now() - START_TS) / 1000;
  if (s < 30)   return "warmup";
  // 30 s warmup + 15 s ramp-up + OVERLOAD_DURATION
  const overloadEnd = 30 + 15 + parseDuration(OVERLOAD_DURATION);
  if (s < overloadEnd) return "overload";
  return "recovery";
}

function parseDuration(d) {
  // Parse simple duration strings like "3m", "180s"
  const m = d.match(/^(\d+)(m|s)$/);
  if (!m) return 180;
  return parseInt(m[1], 10) * (m[2] === "m" ? 60 : 1);
}

// ── Endpoint mix ───────────────────────────────────────────────────────────
const ENDPOINTS = [
  { weight: 60, path: "/v1/events?page=1&limit=20" },
  { weight: 20, path: "/v1/events?page=1&limit=20&event_type=contract" },
  { weight: 10, path: "/v1/events?from_ledger=1000000&to_ledger=1100000&limit=20" },
  { weight: 10, path: "/v1/events?exact_count=true&limit=20" },
];

function pickPath() {
  const total = ENDPOINTS.reduce((s, e) => s + e.weight, 0);
  let r = Math.random() * total;
  for (const e of ENDPOINTS) {
    r -= e.weight;
    if (r <= 0) return e.path;
  }
  return ENDPOINTS[0].path;
}

// ── Main VU function ───────────────────────────────────────────────────────
export default function () {
  const path  = pickPath();
  const res   = http.get(`${BASE_URL}${path}`, { headers: HEADERS, timeout: "15s" });
  const phase = currentPhase();

  latency.add(res.timings.duration);
  throughput.add(1);

  switch (phase) {
    case "warmup":   warmupLatency.add(res.timings.duration);   break;
    case "overload": overloadLatency.add(res.timings.duration); break;
    case "recovery": recoveryLatency.add(res.timings.duration); break;
  }

  // Accept both 200 (normal) and 429 (rate-limited — expected under overload)
  const ok = check(res, {
    "status 200 or 429": (r) => r.status === 200 || r.status === 429,
  });
  errorRate.add(!ok);
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

  const overloadP99 = m.soload_overload_ms?.values?.["p(99)"] ?? null;
  const warmupP99   = m.soload_warmup_ms?.values?.["p(99)"]   ?? null;
  const recovP99    = m.soload_recovery_ms?.values?.["p(99)"] ?? null;

  const sloExceeded = overloadP99 !== null && overloadP99 > 200;
  const errPct      = (m.soload_error_rate?.values?.rate ?? 0) * 100;

  console.log("\n=== SUSTAINED OVERLOAD TEST SUMMARY (observational) ===");
  console.log(`Overload rate    : ${OVERLOAD_RATE} req/s (2× target SLO)`);
  console.log(`Overload duration: ${OVERLOAD_DURATION}`);
  console.log(`Total requests   : ${String(m.soload_requests_total?.values?.count ?? 0)}`);
  console.log(`\nPhase p99 latencies:`);
  console.log(`  Warm-up (100 req/s)      : ${p("soload_warmup_ms",   "p(99)")}`);
  console.log(`  Overload (${OVERLOAD_RATE} req/s)   : ${p("soload_overload_ms", "p(99)")}`);
  console.log(`  Recovery (100 req/s)     : ${p("soload_recovery_ms", "p(99)")}`);
  console.log(`\nOverall:`);
  console.log(`  p95              : ${p("soload_latency_ms", "p(95)")}`);
  console.log(`  p99              : ${p("soload_latency_ms", "p(99)")}`);
  console.log(`  Error rate       : ${pct("soload_error_rate")}`);
  console.log(`\nObservations:`);
  if (sloExceeded) {
    console.log(`  ⚠️  p99 latency (${overloadP99.toFixed(1)} ms) exceeded the 200 ms SLO during overload`);
  } else {
    console.log(`  ✅ p99 latency stayed within SLO even under ${OVERLOAD_RATE} req/s`);
  }
  if (errPct > 1) {
    console.log(`  ⚠️  Error rate (${errPct.toFixed(2)} %) exceeded 1 % under overload`);
  } else {
    console.log(`  ✅ Error rate remained within 1 % under overload`);
  }
  if (recovP99 !== null && recovP99 < 200) {
    console.log(`  ✅ Service recovered to SLO after overload (recovery p99: ${recovP99.toFixed(1)} ms)`);
  } else if (recovP99 !== null) {
    console.log(`  ⚠️  Service has not fully recovered (recovery p99: ${recovP99.toFixed(1)} ms > 200 ms)`);
  }
  console.log(`\nNOTE: This scenario does not fail CI. Results are informational only.`);

  return {
    "tests/load/results/sustained_overload_summary.json": JSON.stringify(data, null, 2),
  };
}
