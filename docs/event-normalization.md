# Event Normalization

This document describes the enhancements made to `src/normalizer.rs` for
normalizing events to a consistent, validated schema.

## Overview

The normalizer already supported applying `NormalizationRule`s (JSON
Pointer + transform + params) to raw event data via `normalize()`. This
work adds:

- **Schema mapping configuration** — `NormalizedSchema` / `SchemaField`
  declare the expected shape of normalized data: which JSON Pointers must
  exist and what `FieldType` (String, Number, Bool, Object, Array, Any)
  they must resolve to.
- **Transformation rules** — unchanged, reuses the existing
  `Transform`/`apply_transform` (divide_by_decimals, hex_to_decimal,
  base64_decode) as the building blocks normalization rules compose.
- **Validation against normalized schema** — `validate_schema(schema,
  value)` checks a normalized value against a `NormalizedSchema` and
  returns a list of `SchemaViolation`s (missing required fields, type
  mismatches).
- **Metrics for normalization** — `NormalizationMetrics` tracks attempted,
  succeeded, rolled-back, and skipped normalization runs, exposing
  `success_rate()`.
- **Rollback on validation failure** — `normalize_with_validation(rules,
  event_data, schema, metrics)` applies normalization rules and then
  validates the result. If validation fails, the original pre-normalization
  data is returned instead (`NormalizationOutcome::RolledBack`) so a
  misconfigured rule can never write malformed data into storage.

## Usage

```rust
use soroban_pulse::normalizer::*;
use serde_json::json;

let rules = vec![/* NormalizationRule { pointer: "/amount", transform: "divide_by_decimals", .. } */];
let schema = NormalizedSchema::new("transfer_event")
    .with_field("/amount", FieldType::Number, true)
    .with_field("/from", FieldType::String, true);

let mut metrics = NormalizationMetrics::default();
let event_data = json!({"amount": 100, "from": "GABC..."});

match normalize_with_validation(&rules, &event_data, &schema, &mut metrics) {
    NormalizationOutcome::Applied(normalized) => store(normalized),
    NormalizationOutcome::RolledBack { original, violations } => {
        tracing::warn!(?violations, "normalization rolled back");
        store(original);
    }
    NormalizationOutcome::Skipped => {}
}

println!("normalization success rate: {:.2}%", metrics.success_rate() * 100.0);
```

## Testing

New unit tests in `src/normalizer.rs` cover schema validation (pass, missing
required field, wrong type), `normalize_with_validation` applying
successfully, rolling back on a schema violation, skipping when there are
no rules, and metrics success-rate computation.
