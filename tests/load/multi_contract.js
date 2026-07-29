// k6 multi-contract query & complex filter load test — issue #811
//
// Tests real-world query patterns:
//   - Per-contract event lookups (hot and cold contracts)
//   - Ledger-range scans with various window sizes
//   - Combined type + ledger-range filters
//   - Transaction-hash lookups
//   - Pagination across large result sets
//   - NDJSON (streaming) export requests
//   - Multiplexed SSE for multiple contracts
//
// Run:  k6 run tests/load/multi_contract.js
// Env:  BASE_URL        (default http://localhost:3000)
//       API_KEY         (optional)
//       CONTRACT_IDS    (comma-separated list; uses built-in stubs if unset)

import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate, Counter } from "k6/metrics";
import { randomIntBetween } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";

// ── Custom metrics ─────────────────────────────────────────────────────────
const contractLatency   = new Trend("mc_contract_latency_ms",    true);
const filterLatency     = new Trend("mc_filter_latency_ms",      true);
const paginationLatency = new Trend("mc_pagination_latency_ms",  true);
const ndjsonLatency     = new Trend("mc_ndjson_latency_ms",      true);
const txLatency         = new Trend("mc_tx_latency_ms",          true);
const errorRate         = new Rate("mc_error_rate");
const throughput        = new Counter("mc_requests_total");

// ── Scenario ───────────────────────────────────────────────────────────────
export const options = {
  scenarios: {
    multi_contract_load: {
      executor: "ramping-arrival-rate",
      startRate: 20,
      timeUnit: "1s",
      stages: [
        { duration: "1m",  target: 50  },   // warm-up
        { duration: "3m",  target: 100 },   // baseline
        { duration: "3m",  target: 200 },   // moderate stress
        { duration: "2m",  target: 100 },   // cool-down
      ],
      preAllocatedVUs: 50,
      maxVUs: 150,
    },
  },
  thresholds: {
    mc_contract_latency_ms:   ["p(99)<300"],
    mc_filter_latency_ms:     ["p(99)<400"],
    mc_pagination_latency_ms: ["p(99)<300"],
    mc_ndjson_latency_ms:     ["p(99)<500"],
    mc_tx_latency_ms:         ["p(99)<300"],
    mc_error_rate:            ["rate<0.01"],
  },
};

const BASE_URL = __ENV.BASE_URL || "http://localhost:3000";
const HEADERS  = __ENV.API_KEY ? { "X-Api-Key": __ENV.API_KEY } : {};

// Use injected contract IDs or fall back to representative stubs.
const RAW_IDS = __ENV.CONTRACT_IDS
  ? __ENV.CONTRACT_IDS.split(",").map((s) => s.trim())
  : [
      "CABC1111111111111111111111111111111111111111111111111111",
      "CDEF2222222222222222222222222222222222222222222222222222",
      "CGHI3333333333333333333333333333333333333333333333333333",
      "CJKL4444444444444444444444444444444444444444444444444444",
      "CMNO5555555555555555555555555555555555555555555555555555",
      "CPQR6666666666666666666666666666666666666666666666666666",
      "CSTU7777777777777777777777777777777777777777777777777777",
      "CVWX8888888888888888888888888888888888888888888888888888",
    ];

// Simulate a hot/cold distribution: first 2 contracts get 60 % of traffic.
function pickContract() {
  return Math.random() < 0.6
    ? RAW_IDS[randomIntBetween(0, 1)]
    : RAW_IDS[randomIntBetween(2, RAW_IDS.length - 1)];
}

// Stub TX hashes for tx-lookup tests (realistic but non-existent — expect 200 + empty data)
const TX_HASHES = [
  "aabbcc001122334455667788990011223344556677889900aabbccdd00112233",
  "ddeeff445566778899001122334455667788990011223344556677889900aabb",
];

// ── Query pattern implementations ──────────────────────────────────────────

function contractQuery() {
  const id  = pickContract();
  const res = http.get(
    `${BASE_URL}/v1/events/${id}?page=1&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  contractLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

function ledgerRangeQuery() {
  // Vary window size to exercise different index scan widths.
  const windowSizes = [100, 1_000, 10_000, 100_000];
  const windowSize  = windowSizes[randomIntBetween(0, windowSizes.length - 1)];
  const fromLedger  = randomIntBetween(500_000, 1_400_000);
  const toLedger    = fromLedger + windowSize;

  const res = http.get(
    `${BASE_URL}/v1/events?from_ledger=${fromLedger}&to_ledger=${toLedger}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  filterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

function combinedFilterQuery() {
  const eventTypes = ["contract", "diagnostic", "system"];
  const type       = eventTypes[randomIntBetween(0, eventTypes.length - 1)];
  const fromLedger = randomIntBetween(900_000, 1_000_000);
  const toLedger   = fromLedger + randomIntBetween(5_000, 50_000);

  const res = http.get(
    `${BASE_URL}/v1/events?event_type=${type}&from_ledger=${fromLedger}&to_ledger=${toLedger}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  filterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

function deepPaginationQuery() {
  // High page numbers stress the OFFSET path.
  const page = randomIntBetween(50, 200);
  const res  = http.get(
    `${BASE_URL}/v1/events?page=${page}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  paginationLatency.add(res.timings.duration);
  throughput.add(1);
  // 200 or 404 (page beyond end) are both acceptable
  errorRate.add(!check(res, { "status 200 or 404": (r) => r.status === 200 || r.status === 404 }));
}

function ndjsonExportQuery() {
  const res = http.get(
    `${BASE_URL}/v1/events?page=1&limit=50`,
    {
      headers: { ...HEADERS, Accept: "application/x-ndjson" },
      timeout: "15s",
    }
  );
  ndjsonLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

function txHashQuery() {
  const hash = TX_HASHES[randomIntBetween(0, TX_HASHES.length - 1)];
  const res  = http.get(
    `${BASE_URL}/v1/events/tx/${hash}`,
    { headers: HEADERS, timeout: "10s" }
  );
  txLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

function multiplexedSseQuery() {
  const ids = [pickContract(), pickContract()].join(",");
  const res = http.get(
    `${BASE_URL}/v1/events/stream/multi?contract_ids=${ids}`,
    {
      headers: { ...HEADERS, Accept: "text/event-stream" },
      timeout: "10s",
    }
  );
  throughput.add(1);
  errorRate.add(!check(res, { "sse status 200": (r) => r.status === 200 }));
  sleep(randomIntBetween(1, 3));
}

// ── Weighted dispatcher ────────────────────────────────────────────────────
const PATTERNS = [
  { weight: 30, fn: contractQuery         },
  { weight: 20, fn: ledgerRangeQuery      },
  { weight: 20, fn: combinedFilterQuery   },
  { weight: 15, fn: deepPaginationQuery   },
  { weight:  8, fn: ndjsonExportQuery     },
  { weight:  5, fn: txHashQuery           },
  { weight:  2, fn: multiplexedSseQuery   },
];

export default function () {
  const total = PATTERNS.reduce((s, p) => s + p.weight, 0);
  let r = Math.random() * total;
  for (const p of PATTERNS) {
    r -= p.weight;
    if (r <= 0) { p.fn(); return; }
  }
  PATTERNS[0].fn();
}

export function handleSummary(data) {
  const fmt = (m, k) => {
    const v = data.metrics[m]?.values?.[k] ?? "n/a";
    return typeof v === "number" ? v.toFixed(1) : v;
  };

  console.log("\n=== MULTI-CONTRACT QUERY SUMMARY ===");
  console.log(`Total requests          : ${data.metrics.mc_requests_total?.values?.count ?? 0}`);
  console.log(`Contract query p99      : ${fmt("mc_contract_latency_ms",   "p(99)")} ms`);
  console.log(`Filter query p99        : ${fmt("mc_filter_latency_ms",     "p(99)")} ms`);
  console.log(`Pagination p99          : ${fmt("mc_pagination_latency_ms", "p(99)")} ms`);
  console.log(`NDJSON export p99       : ${fmt("mc_ndjson_latency_ms",     "p(99)")} ms`);
  console.log(`TX lookup p99           : ${fmt("mc_tx_latency_ms",         "p(99)")} ms`);
  console.log(`Error rate              : ${
    (() => {
      const v = data.metrics.mc_error_rate?.values?.rate ?? "n/a";
      return typeof v === "number" ? (v * 100).toFixed(2) + " %" : v;
    })()
  }`);

  return {
    "tests/load/results/multi_contract_summary.json": JSON.stringify(data, null, 2),
  };
}
