#!/usr/bin/env node
// tests/load/analyze_results.js
//
// Historical trend analysis and baseline promotion tool for Soroban Pulse load tests.
//
// Subcommands:
//   trend   <scenario> [--last N] [--db-size S] [--results-dir D]
//           Print a tabular trend of the last N archived results.
//
//   compare <result1.json> <result2.json> [--scenario S]
//           Side-by-side comparison of two k6 JSON result files.
//
//   promote <scenario> <result.json> [db_size] [--baselines-file F]
//           Extract key metrics from a result file and write them into
//           baselines.json as the new baseline for that scenario/db_size.
//
//   summary <result.json> [--scenario S]
//           Human-readable summary of a single result file.
//
// Exit codes:
//   0  success
//   1  regression / analysis failure
//   2  usage error / file not found

"use strict";

const fs   = require("fs");
const path = require("path");

// ── CLI parsing ───────────────────────────────────────────────────────────────

const [, , subcommand, ...rawArgs] = process.argv;

if (!subcommand || subcommand === "help" || subcommand === "--help") {
  printHelp();
  process.exit(0);
}

// Simple flag parser
function parseArgs(args) {
  const opts   = {};
  const pos    = [];
  let i = 0;
  while (i < args.length) {
    if (args[i].startsWith("--")) {
      const key = args[i].slice(2).replace(/-([a-z])/g, (_, c) => c.toUpperCase());
      opts[key] = args[i + 1] ?? true;
      i += 2;
    } else {
      pos.push(args[i]);
      i++;
    }
  }
  return { opts, pos };
}

const { opts, pos } = parseArgs(rawArgs);

// ── Helpers ───────────────────────────────────────────────────────────────────

function die(msg)  { console.error(`ERROR: ${msg}`); process.exit(2); }
function warn(msg) { console.warn(`WARN: ${msg}`); }

function loadJson(filePath) {
  if (!fs.existsSync(filePath)) die(`file not found: ${filePath}`);
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch (e) {
    die(`failed to parse JSON from ${filePath}: ${e.message}`);
  }
}

function metricVal(data, metricName, statKey) {
  return data?.metrics?.[metricName]?.values?.[statKey] ?? null;
}

function fmtMs(v)  { return v === null ? "n/a" : `${Number(v).toFixed(1)} ms`; }
function fmtPct(v) { return v === null ? "n/a" : `${(Number(v) * 100).toFixed(3)} %`; }
function fmtNum(v) { return v === null ? "n/a" : String(Number(v).toFixed(0)); }

// Per-scenario metric extraction config
const SCENARIO_METRICS = {
  events_steady: [
    { label: "p50 latency",   metric: "events_latency",    stat: "p(50)",  fmt: fmtMs  },
    { label: "p95 latency",   metric: "events_latency",    stat: "p(95)",  fmt: fmtMs  },
    { label: "p99 latency",   metric: "events_latency",    stat: "p(99)",  fmt: fmtMs  },
    { label: "error rate",    metric: "events_errors",      stat: "rate",   fmt: fmtPct },
    { label: "throughput",    metric: "iterations",         stat: "rate",   fmt: fmtNum },
  ],
  sse_stream: [
    { label: "conn p99",      metric: "sse_connection_time",stat: "p(99)",  fmt: fmtMs  },
    { label: "TTFB p99",      metric: "sse_first_byte_time",stat: "p(99)",  fmt: fmtMs  },
    { label: "conn errors",   metric: "sse_connection_errors",stat:"rate",  fmt: fmtPct },
    { label: "churn errors",  metric: "sse_churn_errors",   stat: "rate",   fmt: fmtPct },
  ],
  stress: [
    { label: "p95 latency",   metric: "stress_latency_ms", stat: "p(95)",  fmt: fmtMs  },
    { label: "p99 latency",   metric: "stress_latency_ms", stat: "p(99)",  fmt: fmtMs  },
    { label: "error rate",    metric: "stress_error_rate", stat: "rate",   fmt: fmtPct },
    { label: "total reqs",    metric: "stress_requests_total", stat: "count", fmt: fmtNum },
  ],
  spike: [
    { label: "spike p99",     metric: "spike_latency_ms",          stat: "p(99)", fmt: fmtMs  },
    { label: "recovery p99",  metric: "spike_recovery_latency_ms", stat: "p(99)", fmt: fmtMs  },
    { label: "error rate",    metric: "spike_error_rate",          stat: "rate",  fmt: fmtPct },
  ],
  soak: [
    { label: "p95 latency",   metric: "soak_latency_ms",  stat: "p(95)",  fmt: fmtMs  },
    { label: "p99 latency",   metric: "soak_latency_ms",  stat: "p(99)",  fmt: fmtMs  },
    { label: "error rate",    metric: "soak_error_rate",  stat: "rate",   fmt: fmtPct },
    { label: "total reqs",    metric: "soak_requests_total", stat: "count", fmt: fmtNum },
  ],
  multi_contract: [
    { label: "contract p99",  metric: "mc_contract_latency_ms",   stat: "p(99)", fmt: fmtMs  },
    { label: "filter p99",    metric: "mc_filter_latency_ms",     stat: "p(99)", fmt: fmtMs  },
    { label: "pagination p99",metric: "mc_pagination_latency_ms", stat: "p(99)", fmt: fmtMs  },
    { label: "ndjson p99",    metric: "mc_ndjson_latency_ms",     stat: "p(99)", fmt: fmtMs  },
    { label: "tx p99",        metric: "mc_tx_latency_ms",         stat: "p(99)", fmt: fmtMs  },
    { label: "error rate",    metric: "mc_error_rate",            stat: "rate",  fmt: fmtPct },
  ],
  webhook_delivery: [
    { label: "replay p99",    metric: "wh_replay_latency_ms",  stat: "p(99)", fmt: fmtMs  },
    { label: "metrics p99",   metric: "wh_metrics_latency_ms", stat: "p(99)", fmt: fmtMs  },
    { label: "error rate",    metric: "wh_error_rate",         stat: "rate",  fmt: fmtPct },
  ],
};

function getMetrics(scenario) {
  return SCENARIO_METRICS[scenario] ?? SCENARIO_METRICS["events_steady"];
}

function extractRow(data, scenario) {
  return getMetrics(scenario).map(({ label, metric, stat, fmt }) => ({
    label,
    value: fmt(metricVal(data, metric, stat)),
    raw: metricVal(data, metric, stat),
  }));
}

// ── Subcommand: summary ───────────────────────────────────────────────────────

function cmdSummary(args) {
  const resultFile = args.pos[0] ?? die("Usage: summary <result.json> [--scenario S]");
  const scenario   = args.opts.scenario ?? "events_steady";

  const data = loadJson(resultFile);
  const rows = extractRow(data, scenario);

  console.log(`\n=== SUMMARY: ${path.basename(resultFile)} (${scenario}) ===\n`);
  for (const { label, value } of rows) {
    console.log(`  ${label.padEnd(20)} ${value}`);
  }
  console.log("");
}

// ── Subcommand: compare ───────────────────────────────────────────────────────

function cmdCompare(args) {
  const file1    = args.pos[0] ?? die("Usage: compare <result1.json> <result2.json>");
  const file2    = args.pos[1] ?? die("Usage: compare <result1.json> <result2.json>");
  const scenario = args.opts.scenario ?? "events_steady";

  const d1 = loadJson(file1);
  const d2 = loadJson(file2);

  const rows1 = extractRow(d1, scenario);
  const rows2 = extractRow(d2, scenario);

  const label1 = path.basename(file1);
  const label2 = path.basename(file2);

  console.log(`\n=== COMPARE: ${scenario} ===\n`);
  console.log(
    `  ${"Metric".padEnd(20)} ${"Before".padStart(14)} ${"After".padStart(14)} ${"Delta".padStart(14)}`
  );
  console.log("  " + "─".repeat(66));

  for (let i = 0; i < rows1.length; i++) {
    const r1 = rows1[i];
    const r2 = rows2[i];
    let delta = "—";

    if (r1.raw !== null && r2.raw !== null) {
      const pct = ((r2.raw - r1.raw) / Math.abs(r1.raw)) * 100;
      const sign = pct > 0 ? "+" : "";
      delta = `${sign}${pct.toFixed(1)} %`;
      // Colour-code: regressions for latency/error are positive delta
      if (pct > 10)  delta = `⚠  ${delta}`;
      if (pct > 20)  delta = `✗  ${delta}`;
      if (pct < -5)  delta = `✓  ${delta}`;
    }

    console.log(
      `  ${r1.label.padEnd(20)} ${r1.value.padStart(14)} ${r2.value.padStart(14)} ${delta.padStart(14)}`
    );
  }
  console.log("");
  console.log(`  Before: ${label1}`);
  console.log(`  After:  ${label2}`);
  console.log("");
}

// ── Subcommand: trend ─────────────────────────────────────────────────────────

function cmdTrend(args) {
  const scenario   = args.pos[0] ?? die("Usage: trend <scenario> [--last N] [--db-size S] [--results-dir D]");
  const last       = parseInt(args.opts.last       ?? "10", 10);
  const dbSize     = args.opts.dbSize     ?? null;
  const resultsDir = args.opts.resultsDir ?? "tests/load/results/history";

  const indexFile = path.join(resultsDir, "index.jsonl");

  let entries = [];

  if (fs.existsSync(indexFile)) {
    entries = fs.readFileSync(indexFile, "utf8")
      .split("\n")
      .filter(Boolean)
      .map((line) => { try { return JSON.parse(line); } catch { return null; } })
      .filter(Boolean)
      .filter((e) => e.scenario === scenario)
      .filter((e) => !dbSize || e.db_size === dbSize);
  } else {
    // Fall back to scanning the directory
    const scenarioDir = path.join(resultsDir, scenario);
    if (fs.existsSync(scenarioDir)) {
      entries = fs.readdirSync(scenarioDir)
        .filter((f) => f.endsWith(".json"))
        .sort()
        .map((f) => {
          const parts = f.replace(".json", "").split("_");
          return {
            timestamp: parts[0] ?? f,
            scenario,
            db_size:   parts[1] ?? "unknown",
            file:      path.join(scenarioDir, f),
          };
        })
        .filter((e) => !dbSize || e.db_size === dbSize);
    }
  }

  const recent = entries.slice(-last);

  if (recent.length === 0) {
    console.log(`\nNo archived results found for scenario "${scenario}"${dbSize ? ` (db_size=${dbSize})` : ""}.`);
    console.log(`Run 'scripts/perf_regression.sh archive ${scenario} <result.json>' first.\n`);
    return;
  }

  const metrics = getMetrics(scenario);

  // Determine column widths
  const colW = 14;
  const rowLabelW = 22;

  // Header row
  const tsHeaders = recent.map((e) => {
    const ts = String(e.timestamp).slice(0, 15); // truncate timestamp
    return ts.padStart(colW);
  });

  console.log(`\n=== TREND: ${scenario}${dbSize ? " [" + dbSize + "]" : ""} — last ${recent.length} runs ===\n`);
  console.log(`  ${"Metric".padEnd(rowLabelW)} ${tsHeaders.join(" ")}`);
  console.log("  " + "─".repeat(rowLabelW + (colW + 1) * recent.length));

  for (const { label, metric, stat, fmt } of metrics) {
    const cells = recent.map((entry) => {
      if (!fs.existsSync(entry.file)) return "?".padStart(colW);
      const data = loadJson(entry.file);
      const v    = metricVal(data, metric, stat);
      return fmt(v).padStart(colW);
    });
    console.log(`  ${label.padEnd(rowLabelW)} ${cells.join(" ")}`);
  }

  console.log("");

  // Trend arrow: compare oldest to newest for the primary latency metric
  if (recent.length >= 2) {
    const firstFile = recent[0].file;
    const lastFile  = recent[recent.length - 1].file;
    if (fs.existsSync(firstFile) && fs.existsSync(lastFile)) {
      const d1     = loadJson(firstFile);
      const d2     = loadJson(lastFile);
      const m0     = metrics[0];  // first metric is primary latency
      const v1     = metricVal(d1, m0.metric, m0.stat);
      const v2     = metricVal(d2, m0.metric, m0.stat);
      if (v1 !== null && v2 !== null) {
        const pct = ((v2 - v1) / Math.abs(v1)) * 100;
        const arrow = pct > 10 ? "📈 DEGRADING" : pct < -5 ? "📉 IMPROVING" : "→ STABLE";
        console.log(`  Overall trend (${m0.label}): ${arrow} (${pct >= 0 ? "+" : ""}${pct.toFixed(1)} % over ${recent.length} runs)\n`);
      }
    }
  }
}

// ── Subcommand: promote ───────────────────────────────────────────────────────
// Extract key metrics from a result file and write them as the new baseline.

function cmdPromote(args) {
  const scenario      = args.pos[0] ?? die("Usage: promote <scenario> <result.json> [db_size] [--baselines-file F]");
  const resultFile    = args.pos[1] ?? die("Usage: promote <scenario> <result.json> [db_size]");
  const dbSize        = args.pos[2] ?? "1M";
  const baselinesFile = args.opts.baselinesFile ?? "tests/load/baselines.json";

  const data      = loadJson(resultFile);
  const baselines = loadJson(baselinesFile);

  if (!baselines.scenarios[scenario]) {
    die(`Unknown scenario "${scenario}" in ${baselinesFile}`);
  }

  const scenarioDef = baselines.scenarios[scenario];
  if (!scenarioDef.baselines[dbSize]) {
    // Create placeholder
    scenarioDef.baselines[dbSize] = {};
  }

  const target = scenarioDef.baselines[dbSize];
  const m      = data.metrics ?? {};

  // Helper: extract and round metric value
  const ex = (metricName, stat) => {
    const v = m[metricName]?.values?.[stat] ?? null;
    return v === null ? null : Math.round(v * 100) / 100;
  };

  // Overwrite known keys based on scenario
  switch (scenario) {
    case "events_steady":
      if (ex("events_latency", "p(50)") !== null) target.p50_ms = ex("events_latency", "p(50)");
      if (ex("events_latency", "p(95)") !== null) target.p95_ms = ex("events_latency", "p(95)");
      if (ex("events_latency", "p(99)") !== null) target.p99_ms = ex("events_latency", "p(99)");
      if (ex("events_errors",  "rate")  !== null) target.error_rate = ex("events_errors", "rate");
      break;
    case "stress":
      if (ex("stress_latency_ms", "p(99)") !== null) target.p99_at_1000rps_ms = ex("stress_latency_ms", "p(99)");
      if (ex("stress_latency_ms", "p(95)") !== null) target.p99_at_500rps_ms  = ex("stress_latency_ms", "p(95)");
      if (ex("stress_error_rate",  "rate") !== null) target.error_rate_at_peak = ex("stress_error_rate", "rate");
      break;
    case "spike":
      if (ex("spike_latency_ms",          "p(99)") !== null) target.p99_during_spike_ms    = ex("spike_latency_ms",          "p(99)");
      if (ex("spike_recovery_latency_ms", "p(99)") !== null) target.p99_recovery_ms        = ex("spike_recovery_latency_ms", "p(99)");
      if (ex("spike_error_rate",          "rate")  !== null) target.error_rate_during_spike = ex("spike_error_rate", "rate");
      break;
    case "soak":
      if (ex("soak_latency_ms", "p(95)") !== null) target.p95_ms_hour24 = ex("soak_latency_ms", "p(95)");
      if (ex("soak_latency_ms", "p(99)") !== null) target.p99_ms_hour24 = ex("soak_latency_ms", "p(99)");
      if (ex("soak_error_rate",  "rate") !== null) target.error_rate    = ex("soak_error_rate", "rate");
      break;
    case "multi_contract":
      if (ex("mc_contract_latency_ms",   "p(99)") !== null) target.contract_query_p99_ms = ex("mc_contract_latency_ms",   "p(99)");
      if (ex("mc_filter_latency_ms",     "p(99)") !== null) target.filter_query_p99_ms   = ex("mc_filter_latency_ms",     "p(99)");
      if (ex("mc_pagination_latency_ms", "p(99)") !== null) target.pagination_p99_ms     = ex("mc_pagination_latency_ms", "p(99)");
      if (ex("mc_ndjson_latency_ms",     "p(99)") !== null) target.ndjson_p99_ms         = ex("mc_ndjson_latency_ms",     "p(99)");
      if (ex("mc_tx_latency_ms",         "p(99)") !== null) target.tx_lookup_p99_ms      = ex("mc_tx_latency_ms",         "p(99)");
      if (ex("mc_error_rate",            "rate")  !== null) target.error_rate            = ex("mc_error_rate", "rate");
      break;
    case "webhook_delivery":
      if (ex("wh_replay_latency_ms",  "p(99)") !== null) target.replay_p99_ms         = ex("wh_replay_latency_ms",  "p(99)");
      if (ex("wh_metrics_latency_ms", "p(99)") !== null) target.metrics_probe_p99_ms  = ex("wh_metrics_latency_ms", "p(99)");
      if (ex("wh_error_rate",         "rate")  !== null) target.error_rate            = ex("wh_error_rate", "rate");
      break;
    default:
      die(`No promote logic defined for scenario "${scenario}"`);
  }

  // Append a history record
  baselines.history = (baselines.history ?? []).concat({
    timestamp:   new Date().toISOString(),
    scenario,
    db_size:     dbSize,
    result:      "promoted",
    source_file: resultFile,
  }).slice(-200);

  fs.writeFileSync(baselinesFile, JSON.stringify(baselines, null, 2));

  console.log(`\nBaseline promoted:`);
  console.log(`  Scenario : ${scenario}`);
  console.log(`  DB size  : ${dbSize}`);
  console.log(`  File     : ${baselinesFile}`);
  console.log("\nUpdated values:");
  for (const [k, v] of Object.entries(target)) {
    console.log(`  ${k.padEnd(28)} ${v}`);
  }
  console.log("");
}

// ── Help ──────────────────────────────────────────────────────────────────────

function printHelp() {
  console.log(`
Soroban Pulse — load test result analysis tool (issue #811)

Usage:
  node tests/load/analyze_results.js <subcommand> [args] [options]

Subcommands:
  summary  <result.json> [--scenario S]
      Human-readable summary of a single k6 result file.

  compare  <before.json> <after.json> [--scenario S]
      Side-by-side comparison of two result files with delta percentages.

  trend    <scenario> [--last N] [--db-size S] [--results-dir D]
      Tabular trend across the last N archived runs.
      Scenarios: events_steady, sse_stream, stress, spike, soak,
                 multi_contract, webhook_delivery

  promote  <scenario> <result.json> [db_size] [--baselines-file F]
      Write key metrics from result.json into baselines.json as the new
      baseline for the given scenario and db_size.

Options:
  --scenario     S    Scenario name (default: events_steady)
  --last         N    Number of recent runs to show in trend (default: 10)
  --db-size      S    Filter trend by database size (100k | 1M | 10M)
  --results-dir  D    Path to archived results directory
                      (default: tests/load/results/history)
  --baselines-file F  Path to baselines.json
                      (default: tests/load/baselines.json)

Examples:
  # Summarise a result
  node tests/load/analyze_results.js summary tests/load/results/stress_raw.json \\
    --scenario stress

  # Compare two runs
  node tests/load/analyze_results.js compare \\
    tests/load/results/history/stress/20260101T000000Z_1M.json \\
    tests/load/results/stress_raw.json \\
    --scenario stress

  # Show trend for last 5 multi_contract runs on a 1M dataset
  node tests/load/analyze_results.js trend multi_contract \\
    --last 5 --db-size 1M

  # Promote a result to baseline
  node tests/load/analyze_results.js promote events_steady \\
    tests/load/results/events_steady_raw.json 1M
`);
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

switch (subcommand) {
  case "summary":  cmdSummary({ opts, pos }); break;
  case "compare":  cmdCompare({ opts, pos }); break;
  case "trend":    cmdTrend  ({ opts, pos }); break;
  case "promote":  cmdPromote({ opts, pos }); break;
  default:
    console.error(`Unknown subcommand: ${subcommand}`);
    printHelp();
    process.exit(2);
}
