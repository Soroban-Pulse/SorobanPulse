#!/usr/bin/env bash
# scripts/perf_regression.sh
#
# Historical performance data management for Soroban Pulse load tests.
#
# Responsibilities:
#   1. Archive k6 JSON result files into a dated directory tree
#   2. Run regression_check.js against a new result and exit non-zero on failure
#   3. Purge result files older than a configurable retention period
#   4. Print a tabular trend summary for the last N runs of a scenario
#   5. Update baselines.json with a confirmed "good" result
#
# Usage:
#   scripts/perf_regression.sh archive   <scenario> <result.json> [db_size]
#   scripts/perf_regression.sh check     <scenario> <result.json> [db_size]
#   scripts/perf_regression.sh purge     [--days N]
#   scripts/perf_regression.sh trend     <scenario>              [--last N]
#   scripts/perf_regression.sh promote   <scenario> <result.json> [db_size]
#   scripts/perf_regression.sh help
#
# Environment variables:
#   RESULTS_DIR       Directory to store archived results (default: tests/load/results/history)
#   BASELINES_FILE    Path to baselines.json              (default: tests/load/baselines.json)
#   RETENTION_DAYS    Days to keep old result files       (default: 90)
#
# Exit codes:
#   0  success / no regression
#   1  regression detected
#   2  usage error or missing file

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────
RESULTS_DIR="${RESULTS_DIR:-tests/load/results/history}"
BASELINES_FILE="${BASELINES_FILE:-tests/load/baselines.json}"
RETENTION_DAYS="${RETENTION_DAYS:-90}"
REGRESSION_CHECK="tests/load/regression_check.js"
ANALYZE_RESULTS="tests/load/analyze_results.js"
DATE_TAG="$(date -u +%Y%m%dT%H%M%SZ)"

# ── Helpers ───────────────────────────────────────────────────────────────────
die()  { echo "ERROR: $*" >&2; exit 2; }
info() { echo "  → $*"; }

require_file() { [[ -f "$1" ]] || die "file not found: $1"; }
require_cmd()  { command -v "$1" &>/dev/null || die "required command not found: $1"; }

require_cmd node
require_cmd jq

# ── Subcommand: archive ───────────────────────────────────────────────────────
cmd_archive() {
  local scenario="${1:-}"
  local result_file="${2:-}"
  local db_size="${3:-1M}"

  [[ -n "$scenario"    ]] || die "Usage: archive <scenario> <result.json> [db_size]"
  [[ -n "$result_file" ]] || die "Usage: archive <scenario> <result.json> [db_size]"
  require_file "$result_file"

  local dest_dir="${RESULTS_DIR}/${scenario}"
  mkdir -p "$dest_dir"

  local dest_file="${dest_dir}/${DATE_TAG}_${db_size}.json"
  cp "$result_file" "$dest_file"

  info "Archived: $dest_file"

  # Write a lightweight index entry (scenario / db_size / timestamp / file path)
  local index_file="${RESULTS_DIR}/index.jsonl"
  jq -nc \
    --arg ts  "$DATE_TAG" \
    --arg sc  "$scenario" \
    --arg db  "$db_size" \
    --arg fp  "$dest_file" \
    '{ timestamp: $ts, scenario: $sc, db_size: $db, file: $fp }' \
    >> "$index_file"

  info "Index updated: $index_file"
}

# ── Subcommand: check ─────────────────────────────────────────────────────────
cmd_check() {
  local scenario="${1:-}"
  local result_file="${2:-}"
  local db_size="${3:-1M}"

  [[ -n "$scenario"    ]] || die "Usage: check <scenario> <result.json> [db_size]"
  [[ -n "$result_file" ]] || die "Usage: check <scenario> <result.json> [db_size]"
  require_file "$result_file"
  require_file "$REGRESSION_CHECK"
  require_file "$BASELINES_FILE"

  echo ""
  echo "Running regression check: scenario=${scenario} db_size=${db_size}"
  echo "  Result:    $result_file"
  echo "  Baselines: $BASELINES_FILE"
  echo ""

  # regression_check.js exits 0 (pass) or 1 (regression); let it propagate.
  node "$REGRESSION_CHECK" "$scenario" "$result_file" "$BASELINES_FILE" "$db_size"
}

# ── Subcommand: purge ─────────────────────────────────────────────────────────
cmd_purge() {
  local days="$RETENTION_DAYS"

  # Parse --days N
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --days) days="${2:?--days requires a value}"; shift 2 ;;
      *) die "Unknown option: $1" ;;
    esac
  done

  [[ -d "$RESULTS_DIR" ]] || { info "No results directory found — nothing to purge."; return; }

  echo "Purging result files older than ${days} days from ${RESULTS_DIR} ..."
  local count
  count=$(find "$RESULTS_DIR" -name "*.json" -mtime +"$days" | wc -l)

  if [[ "$count" -eq 0 ]]; then
    info "Nothing to purge."
    return
  fi

  find "$RESULTS_DIR" -name "*.json" -mtime +"$days" -print -delete
  info "Purged ${count} file(s)."
}

# ── Subcommand: trend ─────────────────────────────────────────────────────────
cmd_trend() {
  local scenario="${1:-}"
  local last=10

  [[ -n "$scenario" ]] || die "Usage: trend <scenario> [--last N]"
  shift

  # Parse --last N
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --last) last="${2:?--last requires a value}"; shift 2 ;;
      *) die "Unknown option: $1" ;;
    esac
  done

  # Prefer the Node.js analyzer if available
  if [[ -f "$ANALYZE_RESULTS" ]]; then
    node "$ANALYZE_RESULTS" trend "$scenario" --last "$last" --results-dir "$RESULTS_DIR"
    return
  fi

  # Fallback: simple shell trend from the index file
  local index_file="${RESULTS_DIR}/index.jsonl"
  [[ -f "$index_file" ]] || die "No index file found at ${index_file}. Run 'archive' first."

  echo ""
  echo "=== TREND: ${scenario} (last ${last} runs) ==="
  echo ""

  # Extract the last N entries for this scenario, then for each file show
  # the top-level metric summary available in the raw k6 JSON.
  grep "\"scenario\":\"${scenario}\"" "$index_file" \
    | tail -n "$last" \
    | while IFS= read -r line; do
        local ts fp db
        ts=$(echo "$line" | jq -r '.timestamp')
        db=$(echo "$line" | jq -r '.db_size')
        fp=$(echo "$line" | jq -r '.file')

        if [[ ! -f "$fp" ]]; then
          echo "  ${ts} [${db}]  (file missing: ${fp})"
          continue
        fi

        # Extract a key p99 metric depending on scenario
        local metric_key p99 err
        case "$scenario" in
          events_steady)    metric_key="events_latency"       ;;
          stress)           metric_key="stress_latency_ms"    ;;
          spike)            metric_key="spike_latency_ms"     ;;
          soak)             metric_key="soak_latency_ms"      ;;
          multi_contract)   metric_key="mc_contract_latency_ms" ;;
          webhook_delivery) metric_key="wh_replay_latency_ms" ;;
          *)                metric_key=""                      ;;
        esac

        if [[ -n "$metric_key" ]]; then
          p99=$(jq -r --arg m "$metric_key" \
            '.metrics[$m].values["p(99)"] // "n/a"' "$fp")
          err=$(jq -r '.metrics | to_entries[]
            | select(.key | test("error|err")) | .value.values.rate // empty
            | . * 100 | . * 100 | round / 100 | tostring + " %"' "$fp" \
            | head -1 || echo "n/a")
          if [[ "$p99" != "n/a" ]]; then
            p99=$(printf "%.1f ms" "$p99")
          fi
          printf "  %s  [%s]  p99=%-12s  err=%s\n" "$ts" "$db" "$p99" "${err:-n/a}"
        else
          printf "  %s  [%s]  (no metric mapping defined)\n" "$ts" "$db"
        fi
      done

  echo ""
}

# ── Subcommand: promote ───────────────────────────────────────────────────────
# Update baselines.json with numbers from a known-good result file.
cmd_promote() {
  local scenario="${1:-}"
  local result_file="${2:-}"
  local db_size="${3:-1M}"

  [[ -n "$scenario"    ]] || die "Usage: promote <scenario> <result.json> [db_size]"
  [[ -n "$result_file" ]] || die "Usage: promote <scenario> <result.json> [db_size]"
  require_file "$result_file"
  require_file "$BASELINES_FILE"
  require_file "$ANALYZE_RESULTS"

  echo ""
  echo "Promoting result to baseline: scenario=${scenario} db_size=${db_size}"
  node "$ANALYZE_RESULTS" promote "$scenario" "$result_file" "$db_size" \
    --baselines-file "$BASELINES_FILE"
}

# ── Subcommand: help ──────────────────────────────────────────────────────────
cmd_help() {
  sed -n '/^# Usage:/,/^# Environment/p' "$0" | head -n -1 | sed 's/^# //'
}

# ── Dispatch ──────────────────────────────────────────────────────────────────
SUBCOMMAND="${1:-help}"
shift || true

case "$SUBCOMMAND" in
  archive) cmd_archive "$@" ;;
  check)   cmd_check   "$@" ;;
  purge)   cmd_purge   "$@" ;;
  trend)   cmd_trend   "$@" ;;
  promote) cmd_promote "$@" ;;
  help|--help|-h) cmd_help ;;
  *) die "Unknown subcommand: ${SUBCOMMAND}. Run 'help' for usage." ;;
esac
