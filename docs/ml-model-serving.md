# ML Model Serving

This document describes the model serving infrastructure implemented in
`src/model_serving.rs` for serving ML models against indexed Soroban
contract events, supporting both **event prediction** and **event
classification** workloads.

## Overview

The module provides:

- **Model serving infrastructure** — `ModelRegistry`, the central owner of
  registered model backends, versions, and their runtime stats.
- **Feature extraction** — the `FeatureExtractor` trait plus
  `DefaultFeatureExtractor`, which turns raw event data (`EventFeatureInput`)
  into a `FeatureVector` (numeric + categorical features) ready for
  inference.
- **Model version management** — `ModelVersion` plus
  `ModelRegistry::register_version` / `activate_version`, which supports
  registering multiple versions of a model and promoting exactly one active
  version at a time (safe rollout/rollback).
- **Predictions API** — `ModelRegistry::predict(model_name, input)` extracts
  features, runs inference against the active version's backend, and
  returns a `Prediction` (label, score, latency).
- **Model monitoring** — `evaluate_monitoring` compares a model version's
  live `ModelPerformanceStats` against `MonitoringThresholds` and emits
  `MonitoringAlert`s (`HighErrorRate`, `HighLatency`) suitable for wiring
  into the existing alerting pipeline (`src/alert_manager.rs`).
- **Model performance tracking** — `ModelPerformanceStats` accumulates
  prediction/error counts, latency, and score sums per model version,
  exposing `avg_latency_ms`, `avg_score`, and `error_rate`.

## Usage

```rust
use soroban_pulse::model_serving::*;
use std::sync::Arc;

let registry = ModelRegistry::new(Arc::new(DefaultFeatureExtractor));

registry.register_version(
    ModelVersion::new("fraud-detector", "v1", ModelTask::EventClassification),
    Arc::new(BaselineModel { default_label: "low_risk".into() }),
);
registry.activate_version("fraud-detector", "v1")?;

let input = EventFeatureInput {
    contract_id: "CABCDEF...".into(),
    topic0: Some("transfer".into()),
    ledger_sequence: 123456,
    data_size_bytes: 512,
    successful: true,
};

let prediction = registry.predict("fraud-detector", &input)?;
println!("{} scored {}", prediction.label, prediction.score);

let stats = registry.performance("fraud-detector", "v1").unwrap();
let alerts = evaluate_monitoring("fraud-detector", "v1", &stats, &MonitoringThresholds::default());
```

## Extending

- Implement `ServableModel` to wrap a real inference backend (ONNX Runtime,
  a remote inference microservice, a rule engine, etc.).
- Implement `FeatureExtractor` for task-specific feature sets beyond the
  default (e.g. windowed aggregates, contract-specific decoders).
- Feed `MonitoringAlert`s into `src/alert_manager.rs` on a periodic
  scheduler to page on-call when a served model degrades.

## Testing

Unit tests in `src/model_serving.rs` cover feature extraction, version
activation/rollback, the predictions API happy path and error paths,
performance stat accumulation, and monitoring threshold evaluation.
