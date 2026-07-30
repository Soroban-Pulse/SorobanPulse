#!/usr/bin/env node
// tests/load/lib/baseline.js
// Baseline management library for Soroban Pulse load tests — issue #811
//
// Provides programmatic access to baselines.json:
//   - Load / validate / save the baseline file
//   - Compare a k6 result against a baseline
//   - Update (promote) a baseline from a k6 result
//   - Diff two baselines side-by-side
//
// Can also be used as a CLI:
//
//   node tests/load/lib/baseline.js show [scenario]
//   node tests/load/lib/baseline.js compare <scenario> <result.json> [db_size]
//   node tests/load/lib/baseline.js promote <scenario> <result.json> [db_size] [--dry-run]
//   node tests/load/lib/baseline.js diff <scenario> <db_size1> <db_size2>
//   node tests/load/lib/baseline.js validate

"use strict";

const fs   = require("fs");
const path = require("path");

// ── Constants ──────────────────────────────────────────────────────────────

const DEFAULT_BASELINES_FILE = path.resolve(__dirname, "..", "baselines.json");

/**
 * Per-scenario mapping of k6 metric paths → baseline keys.
 * Format: { metricName, stat, baselineKey, isRate }
 */
const SCENARIO_METRIC_MAP = {
  events_steady: [
    { metricName: "events_latency",  stat: "p(50)", baselineKey: "p50_ms"     },
    { metricName: "events_latency",  stat: "p(95)", baselineKey: "p95_ms"     },
    { metricName: "events_latency",  stat: "p(99)", baselineKey: "p99_ms"     },
    { metricName: "events_errors",   stat: "rate",  baselineKey: "error_rate", isRate: true },
  ],
  sse_stream: [
    { metricName: "sse_connection_time",   stat: "p(99)", baselineKey: "p99_connection_ms" },
    { metricName: "sse_first_byte_time",   stat: "p(99)", baselineKey: "p99_ttfb_ms"       },
    { metricName: "sse_connection_errors", stat: "rate",  baselineKey: "connection_error_rate", isRate: true },
    { metricName: "sse_churn_errors",      stat: "rate",  baselineKey: "churn_error_rate",      isRate: true },
  ],
  stress: [
    { metricName: "stress_latency_ms", stat: "p(99)", baselineKey: "p99_at_1000rps_ms" },
    { metricName: "stress_latency_ms", stat: "p(95)", baselineKey: "p99_at_500rps_ms"  },
    { metricName: "stress_error_rate", stat: "rate",  baselineKey: "error_rate_at_peak", isRate: true },
  ],
  spike: [
    { metricName: "spike_latency_ms",          stat: "p(99)", baselineKey: "p99_during_spike_ms"    },
    { metricName: "spike_recovery_latency_ms", stat: "p(99)", baselineKey: "p99_recovery_ms"        },
    { metricName: "spike_error_rate",          stat: "rate",  baselineKey: "error_rate_during_spike", isRate: true },
  ],
  soak: [
    { metricName: "soak_latency_ms", stat: "p(95)", baselineKey: "p95_ms_hour24" },
    { metricName: "soak_latency_ms", stat: "p(99)", baselineKey: "p99_ms_hour24" },
    { metricName: "soak_error_rate", stat: "rate",  baselineKey: "error_rate",   isRate: true },
  ],
  multi_contract: [
    { metricName: "mc_contract_latency_ms",   stat: "p(99)", baselineKey: "contract_query_p99_ms" },
    { metricName: "mc_filter_latency_ms",     stat: "p(99)", baselineKey: "filter_query_p99_ms"   },
    { metricName: "mc_pagination_latency_ms", stat: "p(99)", baselineKey: "pagination_p99_ms"     },
    { metricName: "mc_ndjson_latency_ms",     stat: "p(99)", baselineKey: "ndjson_p99_ms"         },
    { metricName: "mc_tx_latency_ms",         stat: "p(99)", baselineKey: "tx_lookup_p99_ms"      },
    { metricName: "mc_error_rate",            stat: "rate",  baselineKey: "error_rate",            isRate: true },
  ],
  filter_scenarios: [
    { metricName: "fs_simple_filter_ms",   stat: "p(99)", baselineKey: "simple_filter_p99_ms"   },
    { metricName: "fs_range_filter_ms",    stat: "p(99)", baselineKey: "range_filter_p99_ms"    },
    { metricName: "fs_combined_filter_ms", stat: "p(99)", baselineKey: "combined_filter_p99_ms" },
    { metricName: "fs_exact_count_ms",     stat: "p(99)", baselineKey: "exact_count_p99_ms"     },
    { metricName: "fs_pagination_ms",      stat: "p(99)", baselineKey: "pagination_p99_ms"      },
    { metricName: "fs_contract_filter_ms", stat: "p(99)", baselineKey: "contract_filter_p99_ms" },
    { metricName: "fs_error_rate",         stat: "rate",  baselineKey: "error_rate",             isRate: true },
  ],
  webhook_delivery: [
    { metricName: "wh_replay_latency_ms",  stat: "p(99)", baselineKey: "replay_p99_ms"        },
    { metricName: "wh_metrics_latency_ms", stat: "p(99)", baselineKey: "metrics_probe_p99_ms" },
    { metricName: "wh_error_rate",         stat: "rate",  baselineKey: "error_rate",           isRate: true },
  ],
};

// ── File I/O ───────────────────────────────────────────────────────────────

/**
 * Load and parse the baselines JSON file.
 * @param {string} [filePath]
 * @returns {object}
 */
function loadBaselines(filePath = DEFAULT_BASELINES_FILE) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`Baselines file not found: ${filePath}`);
  }
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

/**
 * Save baselines back to disk with pretty-printing.
 * @param {object} baselines
 * @param {string} [filePath]
 */
function saveBaselines(baselines, filePath = DEFAULT_BASELINES_FILE) {
  fs.writeFileSync(filePath, JSON.stringify(baselines, null, 2) + "\n");
}

/**
 * Load a k6 JSON summary file.
 * @param {string} filePath
 * @returns {object}
 */
function loadSummary(filePath) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`Summary file not found: ${filePath}`);
  }
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

// ── Metric extraction ──────────────────────────────────────────────────────

/**
 * Extract a metric value from a k6 summary object.
 * @param {object} summary
 * @param {string} metricName
 * @param {string} stat
 * @returns {number|null}
 */
function extractMetric(summary, metricName, stat) {
  return summary?.metrics?.[metricName]?.values?.[stat] ?? null;
}

/**
 * Extract all mapped metrics for a scenario from a k6 summary.
 * @param {string} scenario
 * @param {object} summary
 * @returns {Array<{ metricName, stat, baselineKey, isRate, value }>}
 */
function extractScenarioMetrics(scenario, summary) {
  const map = SCENARIO_METRIC_MAP[scenario];
  if (!map) throw new Error(`Unknown scenario: "${scenario}". Available: ${Object.keys(SCENARIO_METRIC_MAP).join(", ")}`);
  return map.map((m) => ({
    ...m,
    value: extractMetric(summary, m.metricName, m.stat),
  }));
}

// ── Comparison ─────────────────────────────────────────────────────────────

/**
 * Compare a k6 result against its baseline.
 *
 * @param {string} scenario
 * @param {object} summary        - parsed k6 JSON summary
 * @param {object} baselines      - parsed baselines.json
 * @param {string} [dbSize="1M"]
 * @returns {{ passes: string[], regressions: string[], skipped: string[] }}
 */
function compareToBaseline(scenario, summary, baselines, dbSize = "1M") {
  const scenarioDef = baselines.scenarios?.[scenario];
  if (!scenarioDef) throw new Error(`Scenario "${scenario}" not found in baselines.`);

  // Support both "1M_events" and "1M" key formats
  const dbEntry = scenarioDef.baselines?.[dbSize]
    ?? scenarioDef.baselines?.[`${dbSize}_events`]
    ?? null;

  if (!dbEntry) {
    throw new Error(`No baseline for db_size="${dbSize}" in scenario "${scenario}".`);
  }

  const thresholds = baselines.regression_thresholds ?? {};
  const LAT_MULT   = thresholds.latency_multiplier          ?? 1.20;
  const ERR_ABS    = thresholds.error_rate_absolute_increase ?? 0.02;

  const passes     = [];
  const regressions = [];
  const skipped    = [];

  const metrics = extractScenarioMetrics(scenario, summary);

  for (const { metricName, stat, baselineKey, isRate, value } of metrics) {
    const baseline = dbEntry[baselineKey];

    if (value === null) {
      skipped.push(`${baselineKey}: metric ${metricName}[${stat}] not found in summary`);
      continue;
    }
    if (baseline === undefined) {
      skipped.push(`${baselineKey}: no baseline value defined`);
      continue;
    }

    let ok;
    let msg;

    if (isRate) {
      const limit = baseline + ERR_ABS;
      ok  = value <= limit;
      msg = `${baselineKey}: ${(value * 100).toFixed(3)}%  (baseline ${(baseline * 100).toFixed(3)}%, limit ${(limit * 100).toFixed(3)}%)`;
    } else {
      const limit = baseline * LAT_MULT;
      ok  = value <= limit;
      msg = `${baselineKey}: ${value.toFixed(1)} ms  (baseline ${baseline} ms, limit ${limit.toFixed(1)} ms)`;
    }

    (ok ? passes : regressions).push(msg);
  }

  return { passes, regressions, skipped };
}

// ── Promotion ──────────────────────────────────────────────────────────────

/**
 * Promote a k6 result as the new baseline for a scenario/dbSize.
 * Returns the updated baselines object without writing to disk.
 *
 * @param {string} scenario
 * @param {object} summary
 * @param {object} baselines
 * @param {string} [dbSize="1M"]
 * @param {string} [sourceFile=""]
 * @returns {{ baselines: object, updated: Record<string,number> }}
 */
function promoteBaseline(scenario, summary, baselines, dbSize = "1M", sourceFile = "") {
  const scenarioDef = baselines.scenarios?.[scenario];
  if (!scenarioDef) throw new Error(`Scenario "${scenario}" not found in baselines.`);

  // Normalise db size key
  const normalKey = dbSize.endsWith("_events") ? dbSize : dbSize;
  if (!scenarioDef.baselines[normalKey]) {
    scenarioDef.baselines[normalKey] = {};
  }
  const target  = scenarioDef.baselines[normalKey];
  const metrics = extractScenarioMetrics(scenario, summary);
  const updated = {};

  for (const { baselineKey, isRate, value } of metrics) {
    if (value === null) continue;
    const rounded = isRate
      ? Math.round(value * 100000) / 100000   // 5 dp for rates
      : Math.round(value * 10) / 10;           // 1 dp for latency ms
    target[baselineKey] = rounded;
    updated[baselineKey] = rounded;
  }

  // Append history record
  baselines.history = (baselines.history ?? []).concat({
    timestamp:   new Date().toISOString(),
    scenario,
    db_size:     dbSize,
    result:      "promoted",
    source_file: sourceFile,
    values:      updated,
  }).slice(-200);

  return { baselines, updated };
}

// ── Validation ─────────────────────────────────────────────────────────────

/**
 * Validate the structure of a baselines.json file.
 * Returns an array of error messages (empty = valid).
 * @param {object} baselines
 * @returns {string[]}
 */
function validateBaselines(baselines) {
  const errors = [];

  if (!baselines.scenarios || typeof baselines.scenarios !== "object") {
    errors.push("Missing or invalid 'scenarios' key");
    return errors;
  }

  for (const [scenario, def] of Object.entries(baselines.scenarios)) {
    if (!def.description) errors.push(`${scenario}: missing description`);
    if (!def.script)      errors.push(`${scenario}: missing script path`);
    if (!def.baselines || typeof def.baselines !== "object") {
      errors.push(`${scenario}: missing or invalid 'baselines' key`);
      continue;
    }

    // Validate that known metric keys are numeric
    for (const [dbSize, vals] of Object.entries(def.baselines)) {
      for (const [k, v] of Object.entries(vals)) {
        if (typeof v !== "number") {
          errors.push(`${scenario}.baselines.${dbSize}.${k}: expected number, got ${typeof v}`);
        }
      }
    }
  }

  if (!baselines.regression_thresholds) {
    errors.push("Missing 'regression_thresholds' section");
  }

  return errors;
}

// ── Diff ────────────────────────────────────────────────────────────────────

/**
 * Produce a side-by-side diff of two db_size baselines within the same scenario.
 *
 * @param {string} scenario
 * @param {object} baselines
 * @param {string} dbSizeA
 * @param {string} dbSizeB
 * @returns {Array<{ key, a, b, delta_pct }>}
 */
function diffBaselines(scenario, baselines, dbSizeA, dbSizeB) {
  const scenarioDef = baselines.scenarios?.[scenario];
  if (!scenarioDef) throw new Error(`Scenario "${scenario}" not found.`);

  const bA = scenarioDef.baselines?.[dbSizeA] ?? {};
  const bB = scenarioDef.baselines?.[dbSizeB] ?? {};
  const allKeys = new Set([...Object.keys(bA), ...Object.keys(bB)]);
  const rows = [];

  for (const key of allKeys) {
    const a = bA[key] ?? null;
    const b = bB[key] ?? null;
    let delta_pct = null;
    if (a !== null && b !== null && a !== 0) {
      delta_pct = ((b - a) / Math.abs(a)) * 100;
    }
    rows.push({ key, a, b, delta_pct });
  }

  return rows.sort((x, y) => x.key.localeCompare(y.key));
}

// ── CLI ────────────────────────────────────────────────────────────────────

if (require.main === module) {
  const [, , subcommand, ...args] = process.argv;

  // Shared option parser
  const hasFlag = (f) => args.includes(f);
  const positional = args.filter((a) => !a.startsWith("--"));

  try {
    switch (subcommand) {

      case "show": {
        const baselines = loadBaselines();
        const filter    = positional[0];
        const scenarios = filter
          ? { [filter]: baselines.scenarios[filter] }
          : baselines.scenarios;

        if (!scenarios[filter ?? Object.keys(scenarios)[0]]) {
          console.error(`Unknown scenario: "${filter}"`);
          process.exit(2);
        }

        for (const [scenario, def] of Object.entries(scenarios)) {
          console.log(`\n── ${scenario} ─────────────────────`);
          console.log(`  Description: ${def.description}`);
          for (const [dbSize, vals] of Object.entries(def.baselines ?? {})) {
            console.log(`  [${dbSize}]`);
            for (const [k, v] of Object.entries(vals)) {
              console.log(`    ${k.padEnd(32)} ${v}`);
            }
          }
        }
        break;
      }

      case "compare": {
        const [scenario, summaryPath, dbSize = "1M"] = positional;
        if (!scenario || !summaryPath) {
          console.error("Usage: baseline.js compare <scenario> <result.json> [db_size]");
          process.exit(2);
        }
        const baselines = loadBaselines();
        const summary   = loadSummary(summaryPath);
        const { passes, regressions, skipped } = compareToBaseline(scenario, summary, baselines, dbSize);

        console.log(`\n=== BASELINE COMPARISON: ${scenario} [${dbSize}] ===\n`);
        if (passes.length)      passes.forEach((m) => console.log(`  ✓ PASS       ${m}`));
        if (skipped.length)     skipped.forEach((m) => console.log(`  ~ SKIP       ${m}`));
        if (regressions.length) regressions.forEach((m) => console.log(`  ✗ REGRESSION ${m}`));

        if (regressions.length > 0) {
          console.log(`\n${regressions.length} regression(s) detected.`);
          process.exit(1);
        } else {
          console.log(`\nAll ${passes.length} checks passed.`);
        }
        break;
      }

      case "promote": {
        const [scenario, summaryPath, dbSize = "1M"] = positional;
        if (!scenario || !summaryPath) {
          console.error("Usage: baseline.js promote <scenario> <result.json> [db_size] [--dry-run]");
          process.exit(2);
        }
        const dryRun    = hasFlag("--dry-run");
        const baselines = loadBaselines();
        const summary   = loadSummary(summaryPath);
        const { baselines: updated, updated: vals } = promoteBaseline(scenario, summary, baselines, dbSize, summaryPath);

        console.log(`\nPromoting baseline: ${scenario} [${dbSize}]`);
        for (const [k, v] of Object.entries(vals)) {
          console.log(`  ${k.padEnd(32)} ${v}`);
        }

        if (dryRun) {
          console.log("\n(dry-run: no changes written)");
        } else {
          saveBaselines(updated);
          console.log(`\nBaseline saved to ${DEFAULT_BASELINES_FILE}`);
        }
        break;
      }

      case "diff": {
        const [scenario, dbSizeA, dbSizeB] = positional;
        if (!scenario || !dbSizeA || !dbSizeB) {
          console.error("Usage: baseline.js diff <scenario> <db_size_a> <db_size_b>");
          process.exit(2);
        }
        const baselines = loadBaselines();
        const rows      = diffBaselines(scenario, baselines, dbSizeA, dbSizeB);

        console.log(`\n=== BASELINE DIFF: ${scenario} [${dbSizeA}] vs [${dbSizeB}] ===\n`);
        console.log(`  ${"Key".padEnd(32)} ${dbSizeA.padStart(14)} ${dbSizeB.padStart(14)} ${"Delta".padStart(12)}`);
        console.log("  " + "─".repeat(76));
        for (const { key, a, b, delta_pct } of rows) {
          const fmtA = a === null ? "n/a" : String(a);
          const fmtB = b === null ? "n/a" : String(b);
          const fmtD = delta_pct === null ? "—" : `${delta_pct >= 0 ? "+" : ""}${delta_pct.toFixed(1)}%`;
          console.log(`  ${key.padEnd(32)} ${fmtA.padStart(14)} ${fmtB.padStart(14)} ${fmtD.padStart(12)}`);
        }
        console.log("");
        break;
      }

      case "validate": {
        const baselines = loadBaselines();
        const errors    = validateBaselines(baselines);
        if (errors.length === 0) {
          console.log(`baselines.json is valid (${Object.keys(baselines.scenarios).length} scenarios).`);
        } else {
          console.error(`baselines.json has ${errors.length} validation error(s):`);
          errors.forEach((e) => console.error(`  - ${e}`));
          process.exit(1);
        }
        break;
      }

      default: {
        console.log(`
Soroban Pulse — baseline management tool (issue #811)

Usage:
  node tests/load/lib/baseline.js <subcommand> [args]

Subcommands:
  show     [scenario]                         Print current baselines
  compare  <scenario> <result.json> [db_size] Compare result against baseline
  promote  <scenario> <result.json> [db_size] [--dry-run]
                                              Promote result as new baseline
  diff     <scenario> <db_size_a> <db_size_b> Diff two database-size baselines
  validate                                    Validate baselines.json structure
`);
        process.exit(subcommand ? 2 : 0);
      }
    }
  } catch (err) {
    console.error(`ERROR: ${err.message}`);
    process.exit(2);
  }
}

// ── Module exports ─────────────────────────────────────────────────────────
module.exports = {
  loadBaselines,
  saveBaselines,
  loadSummary,
  extractMetric,
  extractScenarioMetrics,
  compareToBaseline,
  promoteBaseline,
  validateBaselines,
  diffBaselines,
  SCENARIO_METRIC_MAP,
  DEFAULT_BASELINES_FILE,
};
