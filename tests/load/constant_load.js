// tests/load/constant_load.js — Issue #923
//
// SCENARIO: Constant Load (Steady State)
// ───────────────────────────────────────
// Purpose:  Validate that the service meets its SLOs under a sustained,
//           realistic steady-state workload.  This is the baseline scenario
//           that should always pass before any other load test is trusted.
//
// Pattern:  50 virtual users sending requests continuously for 5 minutes.
//           Requests are distributed across the three most common read paths.
//
// SLO thresholds:
//   - p99 latency  < 200 ms   (matches docs/sli-slo.md target)
//   - error rate   < 1 %
//
// Run:
//   k6 run tests/load/constant_load.js
//   k6 run -e BASE_URL=http://localhost:3000 tests/load/constant_load.js
//   k6 run -e BASE_URL=http://localhost:3000 -e API_KEY=secret tests/load/constant_load.js
//
// Env vars:
//   BASE_URL     Target service base URL (default: http://localhost:3000)
//   API_KEY      Optional API key — sent as X-Api-Key header when set
//   DURATION     Override scenario duration (default: 5m)

import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate, Counter } from "k6/metrics";

// ── Custom metrics ─────────────────────────────────────────────────────────
const latency      = new Trend("cl_latency_ms", true);
const errorRate    = new Rate("cl_error_rate");
const throughput   = new Counter("cl_requests_total");

// Per-endpoint latency breakdown
const eventsListLatency    = new Trend("cl_events_list_ms",     true);
const contractEventsLatency = new Trend("cl_contract_events_ms", true);
const txEventsLatency      = new Trend("cl_tx_events_ms",       true);

// ── Configuration ──────────────────────────────────────────────────────────
const BASE_URL  = (__ENV.BASE_URL  || "http://localhost:3000").replace(/\/$/, "");
const DURATION  = __ENV.DURATION   || "5m";
const HEADERS   = __ENV.API_KEY    ? { "X-Api-Key": __ENV.API_KEY } : {};

// Representative contract IDs and tx hashes used to populate URLs.
// In a real run these should match data seeded in your test database.
const CONTRACT_IDS = [
  "CABC1111111111111111111111111111111111111111111111111111",
  "CDEF2222222222222222222222222222222222222222222222222222",
  "CGHI3333333333333333333333333333333333333333333333333333",
];
const TX_HASHES = [
  "abc1234567890abcdef1234567890abcdef1234567890abcdef1234567890ab",
  "def1234567890abcdef1234567890abcdef1234567890abcdef1234567890cd",
];

// ── Scenario ───────────────────────────────────────────────────────────────
export const options = {
  scenarios: {
    constant_load: {
      executor: "constant-vus",
      vus: 50,
      duration: DURATION,
    },
  },
  thresholds: {
    // Primary SLOs — these must pass for the test to succeed
    cl_latency_ms: [
      { threshold: "p(99)<200", abortOnFail: false },
      { threshold: "p(95)<150", abortOnFail: false },
    ],
    cl_error_rate: [
      { threshold: "rate<0.01", abortOnFail: true },
    ],
    // Per-endpoint SLOs
    cl_events_list_ms:     ["p(99)<200"],
    cl_contract_events_ms: ["p(99)<200"],
    cl_tx_events_ms:       ["p(99)<200"],
  },
};

// ── Endpoint mix (weights must sum to 100) ────────────────────────────────
const ENDPOINTS = [
  // GET /v1/events — primary list endpoint (heaviest traffic)
  { weight: 50, label: "events_list" },
  // GET /v1/events/{contract_id} — per-contract queries
  { weight: 30, label: "contract_events" },
  // GET /v1/events/tx/{tx_hash} — transaction lookup
  { weight: 20, label: "tx_events" },
];

function pickEndpoint() {
  const r = Math.random() * 100;
  let cum = 0;
  for (const e of ENDPOINTS) {
    cum += e.weight;
    if (r < cum) return e.label;
  }
  return ENDPOINTS[0].label;
}

function pickContractId() {
  return CONTRACT_IDS[Math.floor(Math.random() * CONTRACT_IDS.length)];
}

function pickTxHash() {
  return TX_HASHES[Math.floor(Math.random() * TX_HASHES.length)];
}

// ── Main VU function ───────────────────────────────────────────────────────
export default function () {
  const endpoint = pickEndpoint();
  let res;

  switch (endpoint) {
    case "events_list": {
      // Vary page and filter to exercise different query plans
      const page  = Math.floor(Math.random() * 5) + 1;
      const limit = [10, 20, 50][Math.floor(Math.random() * 3)];
      const url   = `${BASE_URL}/v1/events?page=${page}&limit=${limit}`;
      res = http.get(url, { headers: HEADERS, timeout: "10s" });
      eventsListLatency.add(res.timings.duration);
      break;
    }
    case "contract_events": {
      const id  = pickContractId();
      const url = `${BASE_URL}/v1/events/${id}?page=1&limit=20`;
      res = http.get(url, { headers: HEADERS, timeout: "10s" });
      contractEventsLatency.add(res.timings.duration);
      break;
    }
    case "tx_events": {
      const hash = pickTxHash();
      const url  = `${BASE_URL}/v1/events/tx/${hash}`;
      res = http.get(url, { headers: HEADERS, timeout: "10s" });
      txEventsLatency.add(res.timings.duration);
      break;
    }
  }

  latency.add(res.timings.duration);
  throughput.add(1);

  const ok = check(res, {
    "status 200":      (r) => r.status === 200,
    "has body":        (r) => r.body && r.body.length > 0,
    "valid JSON":      (r) => {
      try { JSON.parse(r.body); return true; }
      catch (_) { return false; }
    },
  });
  errorRate.add(!ok);

  // Brief think time — real users don't hammer the API back-to-back
  sleep(Math.random() * 0.5);
}

// ── Summary ────────────────────────────────────────────────────────────────
export function handleSummary(data) {
  const m = data.metrics;
  const p = (metric, stat) => {
    const v = m[metric]?.values?.[stat];
    return v !== undefined ? `${Number(v).toFixed(1)} ms` : "n/a";
  };
  const r = (metric) => {
    const v = m[metric]?.values?.rate;
    return v !== undefined ? `${(Number(v) * 100).toFixed(3)} %` : "n/a";
  };
  const c = (metric) => String(m[metric]?.values?.count ?? 0);

  const p99  = m.cl_latency_ms?.values?.["p(99)"] ?? 0;
  const err  = m.cl_error_rate?.values?.rate       ?? 0;
  const pass = p99 < 200 && err < 0.01;

  console.log("\n=== CONSTANT LOAD TEST SUMMARY ===");
  console.log(`Duration         : ${DURATION}`);
  console.log(`Total requests   : ${c("cl_requests_total")}`);
  console.log(`p95 latency      : ${p("cl_latency_ms", "p(95)")}`);
  console.log(`p99 latency      : ${p("cl_latency_ms", "p(99)")}  (SLO: < 200 ms)`);
  console.log(`Error rate       : ${r("cl_error_rate")}  (SLO: < 1%)`);
  console.log(`\nEndpoint breakdown:`);
  console.log(`  GET /v1/events              p99: ${p("cl_events_list_ms",     "p(99)")}`);
  console.log(`  GET /v1/events/{id}         p99: ${p("cl_contract_events_ms", "p(99)")}`);
  console.log(`  GET /v1/events/tx/{hash}    p99: ${p("cl_tx_events_ms",       "p(99)")}`);
  console.log(`\nResult: ${pass ? "✅ PASS" : "❌ FAIL"}`);

  return {
    "tests/load/results/constant_load_summary.json": JSON.stringify(data, null, 2),
  };
}
