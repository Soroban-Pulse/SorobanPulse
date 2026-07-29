// Soroban Pulse — automated performance regression detection — issue #811
//
// Reads a k6 JSON summary (produced via `handleSummary`) and compares it
// against the baseline library (tests/load/baselines.json).  Exits with a
// non-zero code when any metric regresses beyond the configured thresholds,
// making it suitable as a CI gate.
//
// Usage:
//   node tests/load/regression_check.js <scenario> <summary.json> [baselines.json] [db_size]
//
// Arguments:
//   scenario      — one of: events_steady, sse_stream, stress, spike, soak,
//                            multi_contract, webhook_delivery
//   summary.json  — path to k6 JSON summary output
//   baselines.json — path to baseline library (default: tests/load/baselines.json)
//   db_size        — one of: 100k, 1M, 10M  (default: 1M)
//
// Exit codes:
//   0  — all metrics within thresholds
//   1  — one or more regressions detected
//   2  — bad arguments / missing files

"use strict";

const fs   = require("fs");
const path = require("path");

// ── CLI args ────────────────────────────────────────────────────────────────
const [, , scenario, summaryPath, baselinesPath, dbSize] = process.argv;

if (!scenario || !summaryPath) {
  console.error("Usage: node regression_check.js <scenario> <summary.json> [baselines.json] [db_size]");
  process.exit(2);
}

const BASELINES_FILE = baselinesPath || path.join(__dirname, "baselines.json");
const DB_SIZE        = dbSize        || "1M";

if (!fs.existsSync(summaryPath)) {
  console.error(`ERROR: summary file not found: ${summaryPath}`);
  process.exit(2);
}
if (!fs.existsSync(BASELINES_FILE)) {
  console.error(`ERROR: baselines file not found: ${BASELINES_FILE}`);
  process.exit(2);
}

// ── Load data ───────────────────────────────────────────────────────────────
const summary   = JSON.parse(fs.readFileSync(summaryPath,   "utf8"));
const baselines = JSON.parse(fs.readFileSync(BASELINES_FILE, "utf8"));

if (!baselines.scenarios[scenario]) {
  console.error(`ERROR: unknown scenario "${scenario}". Available: ${Object.keys(baselines.scenarios).join(", ")}`);
  process.exit(2);
}

const scenarioBaseline = baselines.scenarios[scenario];
const dbBaseline       = scenarioBaseline.baselines[DB_SIZE];

if (!dbBaseline) {
  console.error(`ERROR: no baseline for db_size="${DB_SIZE}" in scenario "${scenario}".`);
  process.exit(2);
}

const thresholds = baselines.regression_thresholds;
const LAT_MULT   = thresholds.latency_multiplier        || 1.20;
const ERR_ABS    = thresholds.error_rate_absolute_increase || 0.02;

// ── Metric extraction helpers ───────────────────────────────────────────────

function metricValue(metricName, statKey) {
  return summary.metrics?.[metricName]?.values?.[statKey] ?? null;
}

// ── Regression checks ───────────────────────────────────────────────────────

const regressions = [];
const passes      = [];

function checkLatency(metricName, statKey, baselineKey, label) {
  const actual   = metricValue(metricName, statKey);
  const baseline = dbBaseline[baselineKey];
  if (actual === null || baseline === undefined) {
    console.warn(`  SKIP  ${label}: metric "${metricName}[${statKey}]" not found in summary`);
    return;
  }
  const limit = baseline * LAT_MULT;
  const ok    = actual <= limit;
  const msg   = `${label}: ${actual.toFixed(1)} ms  (baseline ${baseline} ms, limit ${limit.toFixed(1)} ms)`;
  (ok ? passes : regressions).push(msg);
}

function checkErrorRate(metricName, baselineKey, label) {
  const actual   = metricValue(metricName, "rate");
  const baseline = dbBaseline[baselineKey];
  if (actual === null || baseline === undefined) {
    console.warn(`  SKIP  ${label}: metric "${metricName}" not found in summary`);
    return;
  }
  const limit = baseline + ERR_ABS;
  const ok    = actual <= limit;
  const pct   = (v) => (v * 100).toFixed(3) + " %";
  const msg   = `${label}: ${pct(actual)}  (baseline ${pct(baseline)}, limit ${pct(limit)})`;
  (ok ? passes : regressions).push(msg);
}

// ── Per-scenario checks ─────────────────────────────────────────────────────

switch (scenario) {

  case "events_steady":
    checkLatency("events_latency",    "p(99)", "p99_ms",     "events p99 latency");
    checkErrorRate("events_errors",           "error_rate",  "events error rate");
    break;

  case "sse_stream":
    checkLatency("sse_connection_time", "p(99)", "p99_connection_ms", "SSE connection p99");
    checkLatency("sse_first_byte_time", "p(99)", "p99_ttfb_ms",       "SSE TTFB p99");
    checkErrorRate("sse_connection_errors",    "connection_error_rate", "SSE connection errors");
    checkErrorRate("sse_churn_errors",         "churn_error_rate",      "SSE churn errors");
    break;

  case "stress":
    checkLatency("stress_latency_ms", "p(99)", "p99_at_1000rps_ms", "stress peak p99");
    checkLatency("stress_latency_ms", "p(95)", "p99_at_500rps_ms",  "stress mid p95");
    checkErrorRate("stress_error_rate",        "error_rate_at_peak", "stress error rate");
    break;

  case "spike":
    checkLatency("spike_latency_ms",          "p(99)", "p99_during_spike_ms", "spike p99");
    checkLatency("spike_recovery_latency_ms", "p(99)", "p99_recovery_ms",     "recovery p99");
    checkErrorRate("spike_error_rate",                  "error_rate_during_spike", "spike error rate");
    break;

  case "soak":
    checkLatency("soak_latency_ms", "p(95)", "p95_ms_hour24", "soak p95 (hour 24)");
    checkLatency("soak_latency_ms", "p(99)", "p99_ms_hour24", "soak p99 (hour 24)");
    checkErrorRate("soak_error_rate",         "error_rate",    "soak error rate");
    break;

  case "multi_contract":
    checkLatency("mc_contract_latency_ms",   "p(99)", "contract_query_p99_ms", "contract query p99");
    checkLatency("mc_filter_latency_ms",     "p(99)", "filter_query_p99_ms",   "filter query p99");
    checkLatency("mc_pagination_latency_ms", "p(99)", "pagination_p99_ms",     "pagination p99");
    checkLatency("mc_ndjson_latency_ms",     "p(99)", "ndjson_p99_ms",         "NDJSON export p99");
    checkLatency("mc_tx_latency_ms",         "p(99)", "tx_lookup_p99_ms",      "TX lookup p99");
    checkErrorRate("mc_error_rate",                   "error_rate",            "multi-contract error rate");
    break;

  case "webhook_delivery":
    checkLatency("wh_replay_latency_ms",  "p(99)", "replay_p99_ms",         "webhook replay p99");
    checkLatency("wh_metrics_latency_ms", "p(99)", "metrics_probe_p99_ms",  "metrics probe p99");
    checkErrorRate("wh_error_rate",                  "error_rate",           "webhook error rate");
    break;

  default:
    console.error(`ERROR: no checks defined for scenario "${scenario}"`);
    process.exit(2);
}

// ── Report ───────────────────────────────────────────────────────────────────

console.log(`\n=== REGRESSION CHECK: ${scenario.toUpperCase()} (db_size=${DB_SIZE}) ===\n`);

if (passes.length > 0) {
  console.log("PASS:");
  passes.forEach((m) => console.log(`  ✓  ${m}`));
}

if (regressions.length > 0) {
  console.log("\nREGRESSION DETECTED:");
  regressions.forEach((m) => console.log(`  ✗  ${m}`));
  console.log(`\n${regressions.length} regression(s) found. Threshold: >${(LAT_MULT - 1) * 100}% above baseline.`);

  // Append to history in baselines.json for trend tracking
  try {
    const raw    = JSON.parse(fs.readFileSync(BASELINES_FILE, "utf8"));
    const entry  = {
      timestamp:   new Date().toISOString(),
      scenario,
      db_size:     DB_SIZE,
      result:      "regression",
      regressions,
      summary_path: summaryPath,
    };
    raw.history = (raw.history || []).concat(entry).slice(-200); // keep last 200
    fs.writeFileSync(BASELINES_FILE, JSON.stringify(raw, null, 2));
    console.log("\nHistory entry appended to baselines.json.");
  } catch (e) {
    console.warn(`Could not update history: ${e.message}`);
  }

  process.exit(1);
}

// Record passing run in history
try {
  const raw   = JSON.parse(fs.readFileSync(BASELINES_FILE, "utf8"));
  const entry = {
    timestamp:    new Date().toISOString(),
    scenario,
    db_size:      DB_SIZE,
    result:       "pass",
    checks_passed: passes.length,
    summary_path: summaryPath,
  };
  raw.history = (raw.history || []).concat(entry).slice(-200);
  fs.writeFileSync(BASELINES_FILE, JSON.stringify(raw, null, 2));
} catch (e) {
  console.warn(`Could not update history: ${e.message}`);
}

console.log(`\nAll ${passes.length} checks passed. No regressions detected.`);
process.exit(0);
