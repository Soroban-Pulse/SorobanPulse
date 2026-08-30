# Data Quality Monitoring

This document describes the automated data quality checks and reporting
system implemented in `src/data_quality.rs`.

## Overview

- **Rule engine** — `QualityRuleEngine` owns a set of `QualityRule`
  implementations plus batch-level anomaly detectors and runs them against
  a batch of `Record`s (a generic flat field map, decoupled from
  `models::Event` so the engine is reusable for any tabular data).
- **Completeness checks** — `CompletenessRule` flags records where a
  required field is missing or empty.
- **Consistency validation** — `ConsistencyRule` flags records where the
  presence of one field implies another field should also be present
  (e.g. `successful` implies `ledger_sequence` is set).
- **Anomaly detection** — `NumericRangeRule` flags values outside a fixed
  expected range; `ZScoreAnomalyRule` flags values that are statistical
  outliers (beyond N standard deviations) relative to the rest of the
  batch.
- **Metrics** — `QualityMetrics` tracks records checked, violation counts
  by severity, and run count across invocations of the engine, exposing
  `pass_rate()`.
- **Report generation** — `QualityReport` captures a single run's
  violations and pass rate, with `to_text()` for human-readable output
  (e.g. for Slack/email digests).

## Usage

```rust
use soroban_pulse::data_quality::*;

let mut engine = QualityRuleEngine::new();
engine
    .add_rule(Box::new(CompletenessRule { field: "contract_id".into(), severity: Severity::Critical }))
    .add_rule(Box::new(ConsistencyRule {
        if_field: "successful".into(),
        then_field: "ledger_sequence".into(),
        severity: Severity::Warning,
    }))
    .add_anomaly_rule(ZScoreAnomalyRule { field: "data_size_bytes".into(), z_threshold: 3.0, severity: Severity::Warning });

let report = engine.run(&records);
println!("{}", report.to_text());
println!("running pass rate: {:.2}%", engine.metrics().pass_rate() * 100.0);
```

## Integration points

- Run the engine on a schedule against recently ingested events (e.g. from
  `src/indexer.rs` output) and publish `QualityMetrics` via
  `src/metrics.rs`.
- Route `Severity::Critical` violations to `src/alert_manager.rs` for
  paging; route reports to `src/email.rs` / `src/slack.rs` for periodic
  digests.

## Testing

Unit tests in `src/data_quality.rs` cover completeness, consistency,
numeric range, and z-score anomaly detection, as well as end-to-end report
generation and metrics accumulation across multiple engine runs.
