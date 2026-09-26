# Log Analysis Tool

`src/log_analysis_tool.rs` provides an offline analysis pipeline for
troubleshooting from raw log dumps (e.g. `kubectl logs`, a downloaded log
export, or the correlation log ring buffer from
[`docs/correlation-ids.md`](./correlation-ids.md)).

## Pipeline

1. **Parsing** — `parse_log(text)` / `parse_line(line)` extract log level,
   message, optional `correlation_id`, and `timestamp_ms` from each line,
   using a lightweight `key=value` field extractor. Lines that don't match
   the structured shape still parse as `LogLevel::Unknown` rather than being
   dropped.
2. **Error aggregation** — `aggregate_errors(entries)` groups `ERROR` lines
   by a normalized signature (digits collapsed to `#`) so that the same
   underlying error reported with different IDs/ports/hashes counts as one
   bucket, sorted by frequency.
3. **Timeline** — `build_timeline(entries)` returns a chronologically
   sorted list of `(timestamp, level, message)` suitable for feeding into a
   chart/visualization layer.
4. **Correlation detection** — `detect_correlated_groups(entries)` buckets
   entries by `correlation_id` (see `docs/correlation-ids.md`), so every
   line belonging to the same logical request can be reviewed together.
5. **Root cause suggestions** — `suggest_root_causes(groups)` applies a few
   simple heuristics per correlated group (e.g. a `WARN` immediately
   preceding an `ERROR` is flagged as the likely precipitating condition;
   multiple errors in one group suggests a cascading failure) and returns
   suggestions ranked by confidence.
6. **Report generation** — `generate_report(text)` runs the full pipeline
   and returns an `AnalysisReport`; `AnalysisReport::to_text()` renders it as
   a human-readable troubleshooting report.

## Example

```rust
use soroban_pulse::log_analysis_tool::generate_report;

let logs = std::fs::read_to_string("app.log").unwrap();
let report = generate_report(&logs);
println!("{}", report.to_text());
```

Example output:

```
Log Analysis Report
====================
Total lines: 4213
Errors: 37
Correlated groups: 512

Top error signatures:
  [12x] ERROR failed to connect to host #.#.#.#
  [8x]  ERROR upstream timeout after #ms

Root cause suggestions:
  correlation_id=9f2c... confidence=0.6: A warning immediately preceded the error; ...
```

## Testing

`src/log_analysis_tool.rs`'s `tests` module covers line parsing, level
detection, error signature normalization/aggregation, timeline ordering,
correlation grouping, root-cause heuristics, and end-to-end report
generation.
