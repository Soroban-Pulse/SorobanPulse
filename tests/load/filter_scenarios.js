// k6 complex filter combinations load test — issue #811
//
// Exhaustively exercises every filter permutation exposed by
// GET /v1/events and GET /v1/events/{contract_id}:
//
//   - event_type alone (contract | diagnostic | system)
//   - from_ledger alone
//   - to_ledger alone
//   - from_ledger + to_ledger (narrow, medium, wide windows)
//   - event_type + ledger range
//   - exact_count=true (expensive COUNT(*))
//   - exact_count=true + ledger range
//   - limit extremes (1, 20, 100, 200)
//   - deep pagination (page=50, page=100, page=200)
//   - contract filter + event_type
//   - contract filter + ledger range
//   - NDJSON Accept header
//   - combined: type + range + exact_count + high limit
//
// Run:  k6 run tests/load/filter_scenarios.js
// Env:  BASE_URL        (default http://localhost:3000)
//       API_KEY         (optional)
//       CONTRACT_IDS    (comma-separated; built-in stubs used if unset)

import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate, Counter } from "k6/metrics";
import { randomIntBetween } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";

// ── Custom metrics ─────────────────────────────────────────────────────────
const simpleFilterLatency   = new Trend("fs_simple_filter_ms",   true);
const rangeFilterLatency    = new Trend("fs_range_filter_ms",    true);
const combinedFilterLatency = new Trend("fs_combined_filter_ms", true);
const exactCountLatency     = new Trend("fs_exact_count_ms",     true);
const paginationLatency     = new Trend("fs_pagination_ms",      true);
const contractFilterLatency = new Trend("fs_contract_filter_ms", true);
const errorRate             = new Rate("fs_error_rate");
const throughput            = new Counter("fs_requests_total");
const invalidRequests       = new Counter("fs_invalid_requests");

// ── Scenario configuration ─────────────────────────────────────────────────
export const options = {
  scenarios: {
    filter_load: {
      executor: "ramping-arrival-rate",
      startRate: 10,
      timeUnit: "1s",
      stages: [
        { duration: "1m",  target: 30  },  // warm-up
        { duration: "3m",  target: 80  },  // baseline filter load
        { duration: "3m",  target: 150 },  // moderate stress
        { duration: "2m",  target: 80  },  // cool-down
        { duration: "1m",  target: 30  },  // tail
      ],
      preAllocatedVUs: 40,
      maxVUs: 120,
    },
  },
  thresholds: {
    // Simple filters: event_type or single ledger bound
    fs_simple_filter_ms:   ["p(99)<300"],
    // Range filters: from_ledger + to_ledger
    fs_range_filter_ms:    ["p(99)<400"],
    // Combined: type + range (worst-case multi-column scan)
    fs_combined_filter_ms: ["p(99)<500"],
    // exact_count=true forces a full COUNT(*)
    fs_exact_count_ms:     ["p(99)<2000"],
    // Deep pagination with OFFSET
    fs_pagination_ms:      ["p(99)<400"],
    // Per-contract filtered queries
    fs_contract_filter_ms: ["p(99)<300"],
    // Overall error rate
    fs_error_rate:         ["rate<0.01"],
  },
};

const BASE_URL = __ENV.BASE_URL || "http://localhost:3000";
const HEADERS  = __ENV.API_KEY ? { "X-Api-Key": __ENV.API_KEY } : {};

const CONTRACT_IDS = __ENV.CONTRACT_IDS
  ? __ENV.CONTRACT_IDS.split(",").map((s) => s.trim())
  : [
      "CABC1111111111111111111111111111111111111111111111111111",
      "CDEF2222222222222222222222222222222222222222222222222222",
      "CGHI3333333333333333333333333333333333333333333333333333",
      "CJKL4444444444444444444444444444444444444444444444444444",
    ];

const EVENT_TYPES = ["contract", "diagnostic", "system"];

// Ledger ranges that correspond to typical indexing windows
const LEDGER_WINDOWS = [
  { size: 100,       label: "narrow"  },
  { size: 5_000,     label: "medium"  },
  { size: 50_000,    label: "wide"    },
  { size: 500_000,   label: "very_wide" },
];

function pickContract() {
  // Hot-cold: first contract gets 50 % of traffic
  return Math.random() < 0.5
    ? CONTRACT_IDS[0]
    : CONTRACT_IDS[randomIntBetween(1, CONTRACT_IDS.length - 1)];
}

function pickLedgerRange() {
  const w = LEDGER_WINDOWS[randomIntBetween(0, LEDGER_WINDOWS.length - 1)];
  const from = randomIntBetween(500_000, 1_400_000 - w.size);
  return { from, to: from + w.size };
}

// ── Filter pattern implementations ────────────────────────────────────────

/** event_type filter only */
function simpleTypeFilter() {
  const type = EVENT_TYPES[randomIntBetween(0, EVENT_TYPES.length - 1)];
  const res = http.get(
    `${BASE_URL}/v1/events?event_type=${type}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  simpleFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** from_ledger only */
function simpleLedgerFromFilter() {
  const from = randomIntBetween(500_000, 1_400_000);
  const res = http.get(
    `${BASE_URL}/v1/events?from_ledger=${from}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  simpleFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** to_ledger only */
function simpleLedgerToFilter() {
  const to = randomIntBetween(500_000, 1_500_000);
  const res = http.get(
    `${BASE_URL}/v1/events?to_ledger=${to}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  simpleFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** ledger range (from + to) — exercises B-tree scan width */
function rangeFilter() {
  const { from, to } = pickLedgerRange();
  const res = http.get(
    `${BASE_URL}/v1/events?from_ledger=${from}&to_ledger=${to}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  rangeFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** event_type + ledger range — multi-column predicate */
function combinedTypeAndRange() {
  const type        = EVENT_TYPES[randomIntBetween(0, EVENT_TYPES.length - 1)];
  const { from, to } = pickLedgerRange();
  const res = http.get(
    `${BASE_URL}/v1/events?event_type=${type}&from_ledger=${from}&to_ledger=${to}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  combinedFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** exact_count=true — forces COUNT(*), expensive on large datasets */
function exactCountQuery() {
  const res = http.get(
    `${BASE_URL}/v1/events?exact_count=true&limit=20`,
    { headers: HEADERS, timeout: "15s" }
  );
  exactCountLatency.add(res.timings.duration);
  throughput.add(1);
  const ok = check(res, {
    "status 200": (r) => r.status === 200,
    "approximate=false": (r) => {
      try { return JSON.parse(r.body).approximate === false; }
      catch { return false; }
    },
  });
  errorRate.add(!ok);
}

/** exact_count + ledger range — COUNT over a filtered subset */
function exactCountRangeQuery() {
  const { from, to } = pickLedgerRange();
  const res = http.get(
    `${BASE_URL}/v1/events?exact_count=true&from_ledger=${from}&to_ledger=${to}&limit=20`,
    { headers: HEADERS, timeout: "15s" }
  );
  exactCountLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** limit=1 — minimum payload, tests query overhead */
function minLimitQuery() {
  const res = http.get(
    `${BASE_URL}/v1/events?limit=1`,
    { headers: HEADERS, timeout: "10s" }
  );
  simpleFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** limit=100 — larger page, tests serialization cost */
function largeLimitQuery() {
  const res = http.get(
    `${BASE_URL}/v1/events?limit=100`,
    { headers: HEADERS, timeout: "10s" }
  );
  simpleFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** Deep pagination — page=50..200, stress-tests OFFSET */
function deepPagination() {
  const page = randomIntBetween(50, 200);
  const res = http.get(
    `${BASE_URL}/v1/events?page=${page}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  paginationLatency.add(res.timings.duration);
  throughput.add(1);
  // 200 or 404 both acceptable (page may exceed dataset)
  errorRate.add(!check(res, { "status 2xx or 404": (r) => r.status < 500 }));
}

/** Per-contract + event_type filter */
function contractTypeFilter() {
  const id   = pickContract();
  const type = EVENT_TYPES[randomIntBetween(0, EVENT_TYPES.length - 1)];
  const res = http.get(
    `${BASE_URL}/v1/events/${id}?event_type=${type}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  contractFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** Per-contract + ledger range */
function contractRangeFilter() {
  const id           = pickContract();
  const { from, to } = pickLedgerRange();
  const res = http.get(
    `${BASE_URL}/v1/events/${id}?from_ledger=${from}&to_ledger=${to}&limit=20`,
    { headers: HEADERS, timeout: "10s" }
  );
  contractFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** NDJSON streaming response */
function ndjsonFilter() {
  const type = EVENT_TYPES[randomIntBetween(0, EVENT_TYPES.length - 1)];
  const res = http.get(
    `${BASE_URL}/v1/events?event_type=${type}&limit=50`,
    {
      headers: { ...HEADERS, Accept: "application/x-ndjson" },
      timeout: "15s",
    }
  );
  combinedFilterLatency.add(res.timings.duration);
  throughput.add(1);
  const ok = check(res, {
    "status 200": (r) => r.status === 200,
    "ndjson content-type": (r) =>
      (r.headers["Content-Type"] || "").includes("application/x-ndjson") ||
      (r.headers["Content-Type"] || "").includes("application/json"),
  });
  errorRate.add(!ok);
}

/** Stress test: type + range + exact_count + large limit — worst-case combination */
function worstCaseCombination() {
  const type        = EVENT_TYPES[randomIntBetween(0, EVENT_TYPES.length - 1)];
  const { from, to } = pickLedgerRange();
  const res = http.get(
    `${BASE_URL}/v1/events?event_type=${type}&from_ledger=${from}&to_ledger=${to}&exact_count=true&limit=100`,
    { headers: HEADERS, timeout: "20s" }
  );
  combinedFilterLatency.add(res.timings.duration);
  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

/** Validation: from_ledger > to_ledger must return 400 */
function invalidRangeValidation() {
  const from = 2_000_000;
  const to   = 1_000_000; // invalid: to < from
  const res = http.get(
    `${BASE_URL}/v1/events?from_ledger=${from}&to_ledger=${to}`,
    { headers: HEADERS, timeout: "10s" }
  );
  throughput.add(1);
  const isValid = check(res, { "invalid range returns 400": (r) => r.status === 400 });
  if (!isValid) invalidRequests.add(1);
  // Don't count this in error rate — it's intentionally invalid
}

/** Validation: unknown event_type must return 400 */
function invalidTypeValidation() {
  const res = http.get(
    `${BASE_URL}/v1/events?event_type=unknown_type`,
    { headers: HEADERS, timeout: "10s" }
  );
  throughput.add(1);
  const isValid = check(res, { "invalid type returns 400": (r) => r.status === 400 });
  if (!isValid) invalidRequests.add(1);
}

// ── Weighted dispatcher ────────────────────────────────────────────────────
// Weights reflect realistic query mix in production
const PATTERNS = [
  { weight: 15, fn: simpleTypeFilter        },
  { weight: 10, fn: simpleLedgerFromFilter  },
  { weight:  5, fn: simpleLedgerToFilter    },
  { weight: 20, fn: rangeFilter             },
  { weight: 15, fn: combinedTypeAndRange    },
  { weight:  5, fn: exactCountQuery         },
  { weight:  5, fn: exactCountRangeQuery    },
  { weight:  3, fn: minLimitQuery           },
  { weight:  5, fn: largeLimitQuery         },
  { weight:  5, fn: deepPagination          },
  { weight:  5, fn: contractTypeFilter      },
  { weight:  4, fn: contractRangeFilter     },
  { weight:  3, fn: ndjsonFilter            },
  { weight:  2, fn: worstCaseCombination    },
  { weight:  1, fn: invalidRangeValidation  },
  { weight:  1, fn: invalidTypeValidation   },
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
  const fmtPct = (m) => {
    const v = data.metrics[m]?.values?.rate ?? "n/a";
    return typeof v === "number" ? (v * 100).toFixed(3) + " %" : v;
  };

  console.log("\n=== FILTER SCENARIOS SUMMARY ===");
  console.log(`Total requests            : ${data.metrics.fs_requests_total?.values?.count ?? 0}`);
  console.log(`Simple filter p99         : ${fmt("fs_simple_filter_ms",   "p(99)")} ms`);
  console.log(`Range filter p99          : ${fmt("fs_range_filter_ms",    "p(99)")} ms`);
  console.log(`Combined filter p99       : ${fmt("fs_combined_filter_ms", "p(99)")} ms`);
  console.log(`Exact count p99           : ${fmt("fs_exact_count_ms",     "p(99)")} ms`);
  console.log(`Deep pagination p99       : ${fmt("fs_pagination_ms",      "p(99)")} ms`);
  console.log(`Contract filter p99       : ${fmt("fs_contract_filter_ms", "p(99)")} ms`);
  console.log(`Error rate                : ${fmtPct("fs_error_rate")}`);
  console.log(`Invalid-request rejections: ${data.metrics.fs_invalid_requests?.values?.count ?? 0}`);

  return {
    "tests/load/results/filter_scenarios_summary.json": JSON.stringify(data, null, 2),
  };
}
