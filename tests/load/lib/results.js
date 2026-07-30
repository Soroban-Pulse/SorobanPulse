#!/usr/bin/env node
// tests/load/lib/results.js
// Performance result storage and trend analysis — issue #811
//
// Manages the tests/load/results/history/ directory:
//   - Archive a k6 JSON result file into the history store
//   - Query the history index for a scenario
//   - Compute trend statistics across stored results
//   - Detect performance drift over a rolling window
//   - Export a summary report as Markdown or JSON
//
// CLI usage:
//   node tests/load/lib/results.js archive  <scenario> <result.json> [db_size]
//   node tests/load/lib/results.js list     [scenario] [--db-size S] [--last N]
//   node tests/load/lib/results.js trend    <scenario> [--db-size S] [--last N] [--format md|json]
//   node tests/load/lib/results.js clean    [--keep N] [--scenario S] [--dry-run]
//   node tests/load/lib/results.js report   <scenario> [--db-size S] [--format md|json]

"use strict";

const fs   = require("fs");
const path = require("path");

// ── Paths ──────────────────────────────────────────────────────────────────

const RESULTS_ROOT   = path.resolve(__dirname, "..", "results");
const HISTORY_DIR    = path.join(RESULTS_ROOT, "history");
const INDEX_FILE     = path.join(HISTORY_DIR, "index.jsonl");

// ── Per-scenario primary metrics (for trend extraction) ──────────────────

const SCENARIO_METRICS = {
  events_steady: [
    { label: "p50 latency (ms)",  metric: "events_latency",  stat: "p(50)",  isRate: false },
    { label: "p95 latency (ms)",  metric: "events_latency",  stat: "p(95)",  isRate: false },
    { label: "p99 latency (ms)",  metric: "events_latency",  stat: "p(99)",  isRate: false },
    { label: "error rate (%)",    metric: "events_errors",   stat: "rate",   isRate: true  },
    { label: "throughput (rps)",  metric: "iterations",      stat: "rate",   isRate: false },
  ],
  sse_stream: [
    { label: "conn p99 (ms)",     metric: "sse_connection_time",   stat: "p(99)", isRate: false },
    { label: "TTFB p99 (ms)",     metric: "sse_first_byte_time",   stat: "p(99)", isRate: false },
    { label: "conn errors (%)",   metric: "sse_connection_errors", stat: "rate",  isRate: true  },
    { label: "churn errors (%)",  metric: "sse_churn_errors",      stat: "rate",  isRate: true  },
  ],
  stress: [
    { label: "p95 latency (ms)",  metric: "stress_latency_ms",    stat: "p(95)", isRate: false },
    { label: "p99 latency (ms)",  metric: "stress_latency_ms",    stat: "p(99)", isRate: false },
    { label: "error rate (%)",    metric: "stress_error_rate",    stat: "rate",  isRate: true  },
    { label: "total requests",    metric: "stress_requests_total",stat: "count", isRate: false },
  ],
  spike: [
    { label: "spike p99 (ms)",    metric: "spike_latency_ms",          stat: "p(99)", isRate: false },
    { label: "recovery p99 (ms)", metric: "spike_recovery_latency_ms", stat: "p(99)", isRate: false },
    { label: "error rate (%)",    metric: "spike_error_rate",          stat: "rate",  isRate: true  },
  ],
  soak: [
    { label: "p95 latency (ms)",  metric: "soak_latency_ms",  stat: "p(95)", isRate: false },
    { label: "p99 latency (ms)",  metric: "soak_latency_ms",  stat: "p(99)", isRate: false },
    { label: "error rate (%)",    metric: "soak_error_rate",  stat: "rate",  isRate: true  },
    { label: "total requests",    metric: "soak_requests_total", stat: "count", isRate: false },
  ],
  multi_contract: [
    { label: "contract p99 (ms)", metric: "mc_contract_latency_ms",   stat: "p(99)", isRate: false },
    { label: "filter p99 (ms)",   metric: "mc_filter_latency_ms",     stat: "p(99)", isRate: false },
    { label: "pagination p99",    metric: "mc_pagination_latency_ms", stat: "p(99)", isRate: false },
    { label: "ndjson p99 (ms)",   metric: "mc_ndjson_latency_ms",     stat: "p(99)", isRate: false },
    { label: "tx p99 (ms)",       metric: "mc_tx_latency_ms",         stat: "p(99)", isRate: false },
    { label: "error rate (%)",    metric: "mc_error_rate",            stat: "rate",  isRate: true  },
  ],
  filter_scenarios: [
    { label: "simple filter p99", metric: "fs_simple_filter_ms",   stat: "p(99)", isRate: false },
    { label: "range filter p99",  metric: "fs_range_filter_ms",    stat: "p(99)", isRate: false },
    { label: "combined p99",      metric: "fs_combined_filter_ms", stat: "p(99)", isRate: false },
    { label: "exact count p99",   metric: "fs_exact_count_ms",     stat: "p(99)", isRate: false },
    { label: "pagination p99",    metric: "fs_pagination_ms",      stat: "p(99)", isRate: false },
    { label: "error rate (%)",    metric: "fs_error_rate",         stat: "rate",  isRate: true  },
  ],
  webhook_delivery: [
    { label: "replay p99 (ms)",   metric: "wh_replay_latency_ms",  stat: "p(99)", isRate: false },
    { label: "metrics p99 (ms)",  metric: "wh_metrics_latency_ms", stat: "p(99)", isRate: false },
    { label: "error rate (%)",    metric: "wh_error_rate",         stat: "rate",  isRate: true  },
  ],
};

// ── Utilities ──────────────────────────────────────────────────────────────

function ensureDir(dir) {
  if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
}

function die(msg)  { console.error(`ERROR: ${msg}`); process.exit(2); }

function loadJson(filePath) {
  if (!fs.existsSync(filePath)) die(`File not found: ${filePath}`);
  try { return JSON.parse(fs.readFileSync(filePath, "utf8")); }
  catch (e) { die(`JSON parse error in ${filePath}: ${e.message}`); }
}

function extractMetric(summary, metricName, stat) {
  return summary?.metrics?.[metricName]?.values?.[stat] ?? null;
}

function fmtMs(v)  { return v === null ? "n/a" : Number(v).toFixed(1); }
function fmtPct(v) { return v === null ? "n/a" : (Number(v) * 100).toFixed(3); }

// ── Index management ───────────────────────────────────────────────────────

/**
 * Append a record to the newline-delimited index file.
 * @param {object} record
 */
function appendIndex(record) {
  ensureDir(HISTORY_DIR);
  fs.appendFileSync(INDEX_FILE, JSON.stringify(record) + "\n");
}

/**
 * Read all index entries, optionally filtered.
 * @param {{ scenario?: string, dbSize?: string }} [filter]
 * @returns {object[]}
 */
function readIndex({ scenario, dbSize } = {}) {
  if (!fs.existsSync(INDEX_FILE)) return [];
  return fs.readFileSync(INDEX_FILE, "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line) => { try { return JSON.parse(line); } catch { return null; } })
    .filter(Boolean)
    .filter((e) => !scenario || e.scenario === scenario)
    .filter((e) => !dbSize   || e.db_size  === dbSize);
}

// ── Archive ────────────────────────────────────────────────────────────────

/**
 * Archive a k6 JSON result into the history store.
 *
 * @param {string} scenario
 * @param {string} summaryPath
 * @param {string} [dbSize="1M"]
 * @returns {{ archivePath: string, record: object }}
 */
function archiveResult(scenario, summaryPath, dbSize = "1M") {
  const summary   = loadJson(summaryPath);
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-");
  const dirPath   = path.join(HISTORY_DIR, scenario);

  ensureDir(dirPath);

  const filename    = `${timestamp}_${dbSize}.json`;
  const archivePath = path.join(dirPath, filename);

  fs.copyFileSync(summaryPath, archivePath);

  // Extract primary metric values for the index
  const metrics = SCENARIO_METRICS[scenario] ?? [];
  const indexedMetrics = {};
  for (const { label, metric, stat, isRate } of metrics) {
    const v = extractMetric(summary, metric, stat);
    if (v !== null) {
      indexedMetrics[label] = isRate ? fmtPct(v) : fmtMs(v);
    }
  }

  const record = {
    timestamp: new Date().toISOString(),
    scenario,
    db_size:  dbSize,
    file:     archivePath,
    metrics:  indexedMetrics,
  };

  appendIndex(record);
  return { archivePath, record };
}

// ── Trend statistics ───────────────────────────────────────────────────────

/**
 * Compute basic statistics across an array of numeric values.
 * @param {number[]} values
 * @returns {{ min, max, mean, median, p90, latest, trend_pct }}
 */
function computeStats(values) {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const mean   = values.reduce((s, v) => s + v, 0) / values.length;
  const mid    = Math.floor(sorted.length / 2);
  const median = sorted.length % 2 === 0
    ? (sorted[mid - 1] + sorted[mid]) / 2
    : sorted[mid];
  const p90idx = Math.floor(sorted.length * 0.9);
  const p90    = sorted[Math.min(p90idx, sorted.length - 1)];

  const first  = values[0];
  const latest = values[values.length - 1];
  const trend_pct = first !== 0 ? ((latest - first) / Math.abs(first)) * 100 : 0;

  return {
    min:       sorted[0],
    max:       sorted[sorted.length - 1],
    mean:      Math.round(mean * 10) / 10,
    median:    Math.round(median * 10) / 10,
    p90:       Math.round(p90 * 10) / 10,
    latest:    Math.round(latest * 10) / 10,
    trend_pct: Math.round(trend_pct * 10) / 10,
  };
}

/**
 * Build a trend report for a scenario across the last N archived results.
 *
 * @param {string} scenario
 * @param {{ dbSize?: string, last?: number }} [opts]
 * @returns {{ entries: object[], metricTrends: object }}
 */
function buildTrend(scenario, { dbSize, last = 10 } = {}) {
  const entries = readIndex({ scenario, dbSize }).slice(-last);

  if (entries.length === 0) {
    return { entries: [], metricTrends: {} };
  }

  const metrics    = SCENARIO_METRICS[scenario] ?? [];
  const metricTrends = {};

  for (const { label, metric, stat, isRate } of metrics) {
    const values = [];

    for (const entry of entries) {
      if (!fs.existsSync(entry.file)) continue;
      const summary = loadJson(entry.file);
      const raw     = extractMetric(summary, metric, stat);
      if (raw !== null) {
        values.push(isRate ? raw * 100 : raw);  // convert rates to %
      }
    }

    if (values.length > 0) {
      metricTrends[label] = computeStats(values);
    }
  }

  return { entries, metricTrends };
}

/**
 * Detect performance drift: flag if the latest value for the primary metric
 * deviates from the rolling mean by more than `threshold`.
 *
 * @param {string} scenario
 * @param {{ dbSize?: string, last?: number, threshold?: number }} [opts]
 * @returns {{ drifting: boolean, primary: string, mean: number, latest: number, delta_pct: number }|null}
 */
function detectDrift(scenario, { dbSize, last = 10, threshold = 20 } = {}) {
  const { metricTrends } = buildTrend(scenario, { dbSize, last });
  const primaryMetric = SCENARIO_METRICS[scenario]?.[0];
  if (!primaryMetric) return null;

  const stats = metricTrends[primaryMetric.label];
  if (!stats || stats.mean === 0) return null;

  const delta_pct = ((stats.latest - stats.mean) / Math.abs(stats.mean)) * 100;
  return {
    drifting:  Math.abs(delta_pct) > threshold,
    primary:   primaryMetric.label,
    mean:      stats.mean,
    latest:    stats.latest,
    delta_pct: Math.round(delta_pct * 10) / 10,
  };
}

// ── Report formatters ──────────────────────────────────────────────────────

function formatTrendMarkdown(scenario, dbSize, entries, metricTrends) {
  const lines = [];
  lines.push(`# Performance Trend: ${scenario}${dbSize ? ` [${dbSize}]` : ""}`);
  lines.push(`_Generated: ${new Date().toISOString()}_`);
  lines.push(`_Based on last ${entries.length} runs._`);
  lines.push("");

  if (Object.keys(metricTrends).length === 0) {
    lines.push("_No data available._");
    return lines.join("\n");
  }

  lines.push("| Metric | Latest | Mean | Min | Max | Trend |");
  lines.push("|--------|--------|------|-----|-----|-------|");

  for (const [label, stats] of Object.entries(metricTrends)) {
    const isRate = label.includes("error") || label.includes("rate");
    const unit   = isRate ? "%" : "ms";
    const arrow  = stats.trend_pct > 10 ? "📈" : stats.trend_pct < -5 ? "📉" : "→";
    lines.push(
      `| ${label} | ${stats.latest}${unit} | ${stats.mean}${unit} | ${stats.min}${unit} | ${stats.max}${unit} | ${arrow} ${stats.trend_pct >= 0 ? "+" : ""}${stats.trend_pct}% |`
    );
  }

  lines.push("");
  lines.push("## Run History");
  lines.push("");
  lines.push("| Timestamp | DB Size | File |");
  lines.push("|-----------|---------|------|");
  for (const e of entries) {
    lines.push(`| ${e.timestamp.slice(0, 19)} | ${e.db_size} | \`${path.basename(e.file)}\` |`);
  }

  return lines.join("\n");
}

// ── CLI ─────────────────────────────────────────────────────────────────────

if (require.main === module) {
  const [, , subcommand, ...rawArgs] = process.argv;

  const flags    = rawArgs.filter((a) => a.startsWith("--"));
  const positional = rawArgs.filter((a) => !a.startsWith("--"));

  const getFlag  = (name) => {
    const idx = flags.findIndex((f) => f === `--${name}` || f.startsWith(`--${name}=`));
    if (idx === -1) return null;
    if (flags[idx].includes("=")) return flags[idx].split("=")[1];
    // check rawArgs for value
    const rawIdx = rawArgs.indexOf(`--${name}`);
    return rawIdx !== -1 && rawArgs[rawIdx + 1] && !rawArgs[rawIdx + 1].startsWith("--")
      ? rawArgs[rawIdx + 1]
      : true;
  };
  const hasFlag  = (name) => flags.includes(`--${name}`);

  try {
    switch (subcommand) {

      case "archive": {
        const [scenario, summaryPath, dbSize = "1M"] = positional;
        if (!scenario || !summaryPath) die("Usage: results.js archive <scenario> <result.json> [db_size]");
        const { archivePath, record } = archiveResult(scenario, summaryPath, dbSize);
        console.log(`Archived: ${archivePath}`);
        console.log(`Metrics: ${JSON.stringify(record.metrics, null, 2)}`);
        break;
      }

      case "list": {
        const [scenarioFilter] = positional;
        const dbSize = getFlag("db-size") || null;
        const last   = parseInt(getFlag("last") ?? "20", 10);
        const entries = readIndex({ scenario: scenarioFilter || undefined, dbSize })
          .slice(-last);

        if (entries.length === 0) {
          console.log("No results found.");
          break;
        }
        console.log(`\n${"Timestamp".padEnd(25)} ${"Scenario".padEnd(20)} ${"DB Size".padEnd(8)} File`);
        console.log("─".repeat(90));
        for (const e of entries) {
          console.log(`${e.timestamp.slice(0, 19).padEnd(25)} ${e.scenario.padEnd(20)} ${(e.db_size || "").padEnd(8)} ${path.basename(e.file)}`);
        }
        break;
      }

      case "trend": {
        const [scenario] = positional;
        if (!scenario) die("Usage: results.js trend <scenario> [--db-size S] [--last N] [--format md|json]");
        const dbSize  = getFlag("db-size") || null;
        const last    = parseInt(getFlag("last") ?? "10", 10);
        const format  = getFlag("format") || "text";

        const { entries, metricTrends } = buildTrend(scenario, { dbSize, last });

        if (format === "json") {
          console.log(JSON.stringify({ scenario, dbSize, entries: entries.length, metricTrends }, null, 2));
          break;
        }
        if (format === "md") {
          console.log(formatTrendMarkdown(scenario, dbSize, entries, metricTrends));
          break;
        }

        // Default: text table
        console.log(`\n=== TREND: ${scenario}${dbSize ? ` [${dbSize}]` : ""} — last ${entries.length} runs ===\n`);
        if (Object.keys(metricTrends).length === 0) {
          console.log("No data available. Run 'archive' first.\n");
          break;
        }
        const COL = 12;
        console.log(`  ${"Metric".padEnd(24)} ${"Latest".padStart(COL)} ${"Mean".padStart(COL)} ${"Min".padStart(COL)} ${"Max".padStart(COL)} ${"Trend".padStart(COL)}`);
        console.log("  " + "─".repeat(24 + (COL + 1) * 5));
        for (const [label, stats] of Object.entries(metricTrends)) {
          const isRate = label.includes("error") || label.includes("rate");
          const u      = isRate ? "%" : "ms";
          const arrow  = stats.trend_pct > 10 ? "📈" : stats.trend_pct < -5 ? "📉" : "→";
          console.log(
            `  ${label.padEnd(24)} ${(stats.latest + u).padStart(COL)} ${(stats.mean + u).padStart(COL)} ` +
            `${(stats.min + u).padStart(COL)} ${(stats.max + u).padStart(COL)} ` +
            `${(arrow + " " + (stats.trend_pct >= 0 ? "+" : "") + stats.trend_pct + "%").padStart(COL)}`
          );
        }
        console.log("");

        // Drift alert
        const drift = detectDrift(scenario, { dbSize, last });
        if (drift?.drifting) {
          console.log(`⚠  DRIFT ALERT: ${drift.primary} latest=${drift.latest} vs mean=${drift.mean} (${drift.delta_pct >= 0 ? "+" : ""}${drift.delta_pct}%)\n`);
        }
        break;
      }

      case "clean": {
        const keep     = parseInt(getFlag("keep") ?? "50", 10);
        const scenario = getFlag("scenario") || null;
        const dryRun   = hasFlag("dry-run");
        const entries  = readIndex({ scenario });
        const grouped  = {};

        for (const e of entries) {
          const key = `${e.scenario}:${e.db_size}`;
          (grouped[key] = grouped[key] ?? []).push(e);
        }

        let removed = 0;
        for (const [key, group] of Object.entries(grouped)) {
          const toDelete = group.slice(0, Math.max(0, group.length - keep));
          for (const e of toDelete) {
            if (fs.existsSync(e.file)) {
              if (!dryRun) fs.unlinkSync(e.file);
              console.log(`${dryRun ? "[dry-run] would remove" : "removed"}: ${e.file}`);
              removed++;
            }
          }
        }

        if (!dryRun && removed > 0) {
          // Rewrite index without deleted entries
          const surviving = readIndex().filter((e) => fs.existsSync(e.file));
          fs.writeFileSync(INDEX_FILE, surviving.map((e) => JSON.stringify(e)).join("\n") + "\n");
        }

        console.log(`\n${dryRun ? "Would remove" : "Removed"} ${removed} file(s). Kept last ${keep} per scenario/db_size.`);
        break;
      }

      case "report": {
        const [scenario] = positional;
        if (!scenario) die("Usage: results.js report <scenario> [--db-size S] [--format md|json]");
        const dbSize  = getFlag("db-size") || null;
        const format  = getFlag("format") || "md";
        const last    = parseInt(getFlag("last") ?? "10", 10);

        const { entries, metricTrends } = buildTrend(scenario, { dbSize, last });

        if (format === "json") {
          console.log(JSON.stringify({ scenario, dbSize, generated: new Date().toISOString(), entries: entries.length, metricTrends }, null, 2));
        } else {
          console.log(formatTrendMarkdown(scenario, dbSize, entries, metricTrends));
        }
        break;
      }

      default: {
        console.log(`
Soroban Pulse — result storage & trend analysis (issue #811)

Usage:
  node tests/load/lib/results.js <subcommand> [args]

Subcommands:
  archive  <scenario> <result.json> [db_size]        Archive a k6 result
  list     [scenario] [--db-size S] [--last N]       List archived results
  trend    <scenario>  [--db-size S] [--last N]      Show trend statistics
               [--format text|md|json]
  clean    [--keep N] [--scenario S] [--dry-run]     Remove old result files
  report   <scenario>  [--db-size S] [--format ...]  Generate a trend report

Examples:
  node tests/load/lib/results.js archive events_steady tests/load/results/events_steady_raw.json 1M
  node tests/load/lib/results.js trend   stress --db-size 1M --last 5
  node tests/load/lib/results.js report  multi_contract --format md > /tmp/trend.md
  node tests/load/lib/results.js clean   --keep 30 --scenario stress
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
  archiveResult,
  readIndex,
  buildTrend,
  computeStats,
  detectDrift,
  formatTrendMarkdown,
  RESULTS_ROOT,
  HISTORY_DIR,
  INDEX_FILE,
  SCENARIO_METRICS,
};
