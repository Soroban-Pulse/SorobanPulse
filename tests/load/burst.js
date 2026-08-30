// tests/load/burst.js — Issue #923
//
// SCENARIO: Burst (Sudden Traffic Spikes)
// ─────────────────────────────────────────
// Purpose:  Verify that the service survives sudden bursts of traffic — the
//           kind triggered by a popular contract event or a wave of connected
//           clients reconnecting simultaneously after a brief outage.
//
// Pattern:
//   Baseline:    10 VUs continuous throughout the test
//   Burst 1:     Spike to 200 VUs for 30 s (via ramping-arrival-rate)
//   Cooldown 1:  Back to baseline for 1 min
//   Burst 2:     Spike to 200 VUs for 30 s (verifies no resource leak)
//   Cooldown 2:  Back to baseline for 1 min
//
// SLO thresholds:
//   - p99 latency during burst  < 500 ms  (relaxed — system is under shock)
//   - p99 latency during cooldown < 200 ms (should recover to normal SLO)
//   - error rate                < 5 %
//
// Run:
//   k6 run tests/load/burst.js
//   k6 run -e BASE_URL=http://localhost:3000 tests/load/burst.js
//
// Env vars:
//   BASE_URL    Target service base URL (default: http://localhost:3000)
//   API_KEY     Optional API key

import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate, Counter } from "k6/metrics";

// ── Custom metrics ─────────────────────────────────────────────────────────
const latency         = new Trend("burst_latency_ms",      true);
const errorRate       = new Rate("burst_error_rate");
const throughput      = new Counter("burst_requests_total");

// Phase-level breakdown — shows recovery behaviour after each spike
const burstPhaseLatency    = new Trend("burst_spike_ms",    true);
const cooldownPhaseLatency = new Trend("burst_cooldown_ms", true);

// ── Configuration ──────────────────────────────────────────────────────────
const BASE_URL = (__ENV.BASE_URL || "http://localhost:3000").replace(/\/$/, "");
const HEADERS  = __ENV.API_KEY   ? { "X-Api-Key": __ENV.API_KEY } : {};

// ── Scenario ───────────────────────────────────────────────────────────────
// Two concurrent scenarios:
//  1. baseline_vus  — constant 10 VUs throughout (background traffic)
//  2. burst_arrival — ramping-arrival-rate that ramps to 200 req/s during spikes
export const options = {
  scenarios: {
    // Steady background load to simulate "there is always real traffic"
    baseline_vus: {
      executor: "constant-vus",
      vus: 10,
      duration: "4m",
      exec: "baselineRequest",
    },

    // Two spike bursts using ramping-arrival-rate.
    // Total duration: 0 → burst1 (30s) → cooldown (60s) → burst2 (30s) → cooldown (60s)
    burst_arrival: {
      executor: "ramping-arrival-rate",
      startRate: 10,
      timeUnit: "1s",
      stages: [
        // Burst 1: instant ramp to 200 req/s, hold 30 s
        { duration: "5s",  target: 200  },
        { duration: "25s", target: 200  },
        // Cooldown: drop back to baseline
        { duration: "5s",  target: 10   },
        { duration: "55s", target: 10   },
        // Burst 2: same as burst 1 — verify no resource leaks
        { duration: "5s",  target: 200  },
        { duration: "25s", target: 200  },
        // Final cooldown
        { duration: "5s",  target: 10   },
        { duration: "55s", target: 10   },
      ],
      preAllocatedVUs: 50,
      maxVUs: 300,
      exec: "burstRequest",
    },
  },

  thresholds: {
    burst_latency_ms: [
      // Overall — includes both burst and cooldown periods
      { threshold: "p(99)<500",  abortOnFail: false },
      { threshold: "p(95)<300",  abortOnFail: false },
    ],
    burst_error_rate: [
      // 5 % tolerance during spikes (rate-limiter 429s are expected)
      { threshold: "rate<0.05",  abortOnFail: true  },
    ],
    burst_spike_ms: [
      { threshold: "p(99)<500",  abortOnFail: false },
    ],
    burst_cooldown_ms: [
      // After a burst the service should recover to its normal SLO
      { threshold: "p(99)<200",  abortOnFail: false },
    ],
  },
};

// ── Phase tracking ─────────────────────────────────────────────────────────
const START_TS = Date.now();

function isBurstPhase() {
  const s = (Date.now() - START_TS) / 1000;
  // Burst 1: 0–30 s,  Burst 2: 90–120 s
  return (s >= 0 && s < 30) || (s >= 90 && s < 120);
}

// ── Request functions ──────────────────────────────────────────────────────
function makeRequest(url) {
  return http.get(url, { headers: HEADERS, timeout: "15s" });
}

function recordResult(res) {
  latency.add(res.timings.duration);
  throughput.add(1);

  if (isBurstPhase()) {
    burstPhaseLatency.add(res.timings.duration);
  } else {
    cooldownPhaseLatency.add(res.timings.duration);
  }

  const ok = check(res, {
    // Accept 200 (success) and 429 (rate-limited — expected during bursts)
    "status 200 or 429": (r) => r.status === 200 || r.status === 429,
  });
  errorRate.add(!ok);
}

export function baselineRequest() {
  const res = makeRequest(`${BASE_URL}/v1/events?page=1&limit=20`);
  recordResult(res);
  sleep(0.5 + Math.random() * 0.5);
}

export function burstRequest() {
  // Mix of endpoints during burst — simulates diverse client behaviour
  const paths = [
    `/v1/events?page=1&limit=20`,
    `/v1/events?page=1&limit=20&event_type=contract`,
    `/v1/events?from_ledger=1000000&limit=20`,
    `/healthz/ready`,
  ];
  const path = paths[Math.floor(Math.random() * paths.length)];
  const res  = makeRequest(`${BASE_URL}${path}`);
  recordResult(res);
  // No sleep in burst — we want maximum throughput during the spike
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

  const overallP99 = m.burst_latency_ms?.values?.["p(99)"] ?? 0;
  const err        = m.burst_error_rate?.values?.rate       ?? 0;
  const spikeP99   = m.burst_spike_ms?.values?.["p(99)"]    ?? 0;
  const coolP99    = m.burst_cooldown_ms?.values?.["p(99)"]  ?? 0;
  const pass       = overallP99 < 500 && err < 0.05;

  console.log("\n=== BURST TEST SUMMARY ===");
  console.log(`Pattern          : 10 VU baseline + two 200 req/s spikes (30 s each)`);
  console.log(`Total requests   : ${String(m.burst_requests_total?.values?.count ?? 0)}`);
  console.log(`\nOverall:`);
  console.log(`  p95            : ${p("burst_latency_ms", "p(95)")}`);
  console.log(`  p99            : ${p("burst_latency_ms", "p(99)")}  (SLO during burst: < 500 ms)`);
  console.log(`  error rate     : ${pct("burst_error_rate")}  (SLO: < 5 %)`);
  console.log(`\nPhase breakdown:`);
  console.log(`  Spike p99      : ${p("burst_spike_ms",    "p(99)")}`);
  console.log(`  Cooldown p99   : ${p("burst_cooldown_ms", "p(99)")}  (SLO: < 200 ms)`);

  const recovered = coolP99 < 200;
  console.log(`\nRecovery         : ${recovered ? "✅ Service recovered to SLO after burst" : "⚠️  Cooldown p99 still elevated"}`);
  console.log(`\nResult: ${pass ? "✅ PASS" : "❌ FAIL"}`);

  return {
    "tests/load/results/burst_summary.json": JSON.stringify(data, null, 2),
  };
}
