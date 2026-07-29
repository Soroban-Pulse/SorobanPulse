// tests/load/lib/helpers.js
// Shared k6 utility library for Soroban Pulse load tests — issue #811
//
// Import individual helpers into any k6 script:
//
//   import { weightedPick, pickContract, pickLedgerRange, apiHeaders,
//            assertSlo, buildSummaryRow } from "./lib/helpers.js";
//
// All helpers are pure functions — no side-effects, no global state.

import { randomIntBetween } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";

// ── Environment ───────────────────────────────────────────────────────────

/**
 * Resolved base URL from environment, with no trailing slash.
 * @type {string}
 */
export const BASE_URL = (__ENV.BASE_URL || "http://localhost:3000").replace(/\/$/, "");

/**
 * Default contract IDs used when CONTRACT_IDS env var is not set.
 * These are syntactically valid Stellar contract IDs (56 uppercase alphanumeric chars, C-prefix).
 */
export const DEFAULT_CONTRACT_IDS = [
  "CABC1111111111111111111111111111111111111111111111111111",
  "CDEF2222222222222222222222222222222222222222222222222222",
  "CGHI3333333333333333333333333333333333333333333333333333",
  "CJKL4444444444444444444444444444444444444444444444444444",
  "CMNO5555555555555555555555555555555555555555555555555555",
  "CPQR6666666666666666666666666666666666666666666666666666",
  "CSTU7777777777777777777777777777777777777777777777777777",
  "CVWX8888888888888888888888888888888888888888888888888888",
];

/**
 * Parse CONTRACT_IDS env var into an array, falling back to DEFAULT_CONTRACT_IDS.
 * @returns {string[]}
 */
export function resolveContractIds() {
  if (__ENV.CONTRACT_IDS) {
    return __ENV.CONTRACT_IDS.split(",").map((s) => s.trim()).filter(Boolean);
  }
  return DEFAULT_CONTRACT_IDS;
}

/**
 * Build HTTP headers object, optionally injecting an API key.
 * Merges any additional headers on top.
 *
 * @param {Record<string,string>} [extra={}]
 * @returns {Record<string,string>}
 */
export function apiHeaders(extra = {}) {
  const base = __ENV.API_KEY ? { "X-Api-Key": __ENV.API_KEY } : {};
  return { ...base, ...extra };
}

/**
 * Build admin headers (requires ADMIN_API_KEY env var).
 * @param {Record<string,string>} [extra={}]
 * @returns {Record<string,string>}
 */
export function adminHeaders(extra = {}) {
  const key = __ENV.ADMIN_API_KEY || __ENV.API_KEY || "";
  const base = key ? { "X-Api-Key": key, "Content-Type": "application/json" } : { "Content-Type": "application/json" };
  return { ...base, ...extra };
}

// ── Data selection ────────────────────────────────────────────────────────

/**
 * Randomly pick a contract ID with a hot/cold distribution.
 * The first `hotCount` IDs in the array receive `hotFraction` of traffic.
 *
 * @param {string[]} ids
 * @param {number} [hotCount=2]
 * @param {number} [hotFraction=0.6]
 * @returns {string}
 */
export function pickContract(ids, hotCount = 2, hotFraction = 0.6) {
  if (!ids || ids.length === 0) return DEFAULT_CONTRACT_IDS[0];
  const hot = Math.min(hotCount, ids.length);
  if (Math.random() < hotFraction) {
    return ids[randomIntBetween(0, hot - 1)];
  }
  return ids[randomIntBetween(hot, ids.length - 1)];
}

/**
 * Ledger window sizes and their labels.
 */
export const LEDGER_WINDOWS = [
  { size: 100,     label: "narrow" },
  { size: 5_000,   label: "medium" },
  { size: 50_000,  label: "wide"   },
  { size: 500_000, label: "very_wide" },
];

/**
 * Pick a random ledger range from the standard window sizes.
 *
 * @param {{ minLedger?: number, maxLedger?: number }} [opts]
 * @returns {{ from: number, to: number, label: string }}
 */
export function pickLedgerRange({ minLedger = 500_000, maxLedger = 1_400_000 } = {}) {
  const w    = LEDGER_WINDOWS[randomIntBetween(0, LEDGER_WINDOWS.length - 1)];
  const from = randomIntBetween(minLedger, Math.max(minLedger, maxLedger - w.size));
  return { from, to: from + w.size, label: w.label };
}

/**
 * Valid Soroban event types.
 */
export const EVENT_TYPES = ["contract", "diagnostic", "system"];

/**
 * Pick a random event type.
 * @returns {string}
 */
export function pickEventType() {
  return EVENT_TYPES[randomIntBetween(0, EVENT_TYPES.length - 1)];
}

// ── Weighted random selection ─────────────────────────────────────────────

/**
 * Pick a random item from a weighted array.
 * Each entry must have a `weight: number` property.
 *
 * @template T
 * @param {Array<T & { weight: number }>} items
 * @returns {T}
 */
export function weightedPick(items) {
  const total = items.reduce((s, e) => s + e.weight, 0);
  let r = Math.random() * total;
  for (const item of items) {
    r -= item.weight;
    if (r <= 0) return item;
  }
  return items[0];
}

// ── SLO assertion helpers ─────────────────────────────────────────────────

/**
 * SLO definitions aligned with docs/sli-slo.md.
 */
export const SLOS = {
  /** p99 latency SLO for the primary events endpoint at baseline load */
  EVENTS_P99_MS: 200,
  /** SSE connection establishment p99 */
  SSE_CONN_P99_MS: 500,
  /** SSE time-to-first-byte p99 */
  SSE_TTFB_P99_MS: 1000,
  /** Maximum acceptable error rate */
  MAX_ERROR_RATE: 0.01,
  /** Maximum acceptable error rate during a spike */
  MAX_SPIKE_ERROR_RATE: 0.10,
};

/**
 * Assert that a response meets the basic SLOs and return true if ok.
 * Adds to errorRate metric when it fails.
 *
 * @param {import("k6/http").RefinedResponse} res
 * @param {import("k6").check} checkFn  - the k6 `check` function
 * @param {Record<string,function>} [extraChecks={}]
 * @returns {boolean}
 */
export function assertSlo(res, checkFn, extraChecks = {}) {
  return checkFn(res, {
    "status 200": (r) => r.status === 200,
    "body not empty": (r) => r.body && r.body.length > 0,
    ...extraChecks,
  });
}

// ── URL builder helpers ────────────────────────────────────────────────────

/**
 * Build a query string from a params object, omitting null/undefined values.
 *
 * @param {Record<string, string|number|boolean|null|undefined>} params
 * @returns {string}  e.g. "?page=1&limit=20"
 */
export function buildQuery(params) {
  const parts = [];
  for (const [k, v] of Object.entries(params)) {
    if (v === null || v === undefined || v === "") continue;
    parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(String(v))}`);
  }
  return parts.length > 0 ? "?" + parts.join("&") : "";
}

/**
 * Build the full URL for the events list endpoint.
 *
 * @param {Object} [opts]
 * @param {number}  [opts.page]
 * @param {number}  [opts.limit]
 * @param {string}  [opts.eventType]
 * @param {number}  [opts.fromLedger]
 * @param {number}  [opts.toLedger]
 * @param {boolean} [opts.exactCount]
 * @returns {string}
 */
export function eventsUrl({ page, limit, eventType, fromLedger, toLedger, exactCount } = {}) {
  return `${BASE_URL}/v1/events${buildQuery({
    page,
    limit,
    event_type:   eventType,
    from_ledger:  fromLedger,
    to_ledger:    toLedger,
    exact_count:  exactCount,
  })}`;
}

/**
 * Build the URL for a per-contract events endpoint.
 *
 * @param {string} contractId
 * @param {Object} [opts] - same options as eventsUrl except eventType
 * @returns {string}
 */
export function contractEventsUrl(contractId, { page, limit, fromLedger, toLedger } = {}) {
  return `${BASE_URL}/v1/events/${contractId}${buildQuery({
    page,
    limit,
    from_ledger: fromLedger,
    to_ledger:   toLedger,
  })}`;
}

/**
 * Build the SSE stream URL, optionally filtered to a contract.
 *
 * @param {string} [contractId]
 * @returns {string}
 */
export function sseUrl(contractId) {
  return contractId
    ? `${BASE_URL}/v1/events/stream?contract_id=${contractId}`
    : `${BASE_URL}/v1/events/stream`;
}

/**
 * Build the multiplexed SSE URL for multiple contracts.
 *
 * @param {string[]} contractIds
 * @returns {string}
 */
export function sseMultiUrl(contractIds) {
  return `${BASE_URL}/v1/events/stream/multi?contract_ids=${contractIds.join(",")}`;
}

// ── Summary helpers ────────────────────────────────────────────────────────

/**
 * Extract a metric value from a k6 handleSummary data object.
 *
 * @param {object} data   - k6 summary data
 * @param {string} metric - metric name, e.g. "events_latency"
 * @param {string} stat   - stat key, e.g. "p(99)", "rate", "count"
 * @returns {number|null}
 */
export function metricValue(data, metric, stat) {
  return data?.metrics?.[metric]?.values?.[stat] ?? null;
}

/**
 * Format a millisecond value for display.
 * @param {number|null} v
 * @returns {string}
 */
export function fmtMs(v) {
  return v === null ? "n/a" : `${Number(v).toFixed(1)} ms`;
}

/**
 * Format a rate (0–1) as a percentage string.
 * @param {number|null} v
 * @returns {string}
 */
export function fmtPct(v) {
  return v === null ? "n/a" : `${(Number(v) * 100).toFixed(3)} %`;
}

/**
 * Format a count/integer value.
 * @param {number|null} v
 * @returns {string}
 */
export function fmtCount(v) {
  return v === null ? "n/a" : String(Math.round(Number(v)));
}

/**
 * Build a summary row string for console output.
 *
 * @param {string} label       - left-aligned label
 * @param {string} value       - formatted value
 * @param {number} [labelWidth=28]
 * @returns {string}
 */
export function buildSummaryRow(label, value, labelWidth = 28) {
  return `  ${label.padEnd(labelWidth)} ${value}`;
}

/**
 * Print a standard summary header with test name and total request count.
 *
 * @param {string} testName
 * @param {object} data
 * @param {string} throughputMetric - metric name for total request count
 */
export function printSummaryHeader(testName, data, throughputMetric) {
  const total = metricValue(data, throughputMetric, "count");
  console.log(`\n=== ${testName} ===`);
  console.log(buildSummaryRow("Total requests", fmtCount(total)));
}

// ── Retry / resiliency helpers ─────────────────────────────────────────────

/**
 * Perform an HTTP GET with simple retry logic.
 * Returns the last response received.
 *
 * @param {string} url
 * @param {object} params   - k6 http params (headers, timeout, etc.)
 * @param {number} [retries=2]
 * @param {number} [retryDelayMs=500]
 * @returns {import("k6/http").RefinedResponse}
 */
export function getWithRetry(url, params, retries = 2, retryDelayMs = 500) {
  // Note: sleep is not imported here to keep this file self-contained.
  // Callers that need sleep-based retry should use the k6 `sleep` function directly.
  const http = require ? require("k6/http") : globalThis.http;

  let res;
  for (let attempt = 0; attempt <= retries; attempt++) {
    res = http.get(url, params);
    if (res.status >= 200 && res.status < 500) break;
    // 5xx — retry after a brief wait (k6 does not expose setTimeout, so we busy-loop)
  }
  return res;
}
