# Subscription Configuration Validator

A pre-deployment validation tool for Soroban Pulse subscription configurations.
It catches malformed filters, incompatible schema versions, unsafe
transformations, and missing/unsafe resource limits before a subscription is
deployed to a running indexer.

Implemented in [`src/subscription_validator.rs`](../src/subscription_validator.rs).

## Why

Subscription configurations are typically authored as JSON/YAML and applied
directly to a running deployment. Mistakes (unbalanced filter brackets, a
schema version the deployment doesn't support, an unbounded rate limit) are
otherwise only discovered at runtime, often after events have already started
flowing. This tool lets CI or a local `cargo run --bin subscription-validator`
step catch these problems ahead of time.

## Architecture

- **`ValidationRule`** — a trait implemented by each individual check. Rules
  are stateless and operate on a single `SubscriptionConfig`.
- **`ValidationEngine`** — a rule registry that runs every registered rule
  against a config (or a batch of configs) and aggregates the resulting
  `ValidationFinding`s into a `ValidationReport`.
- **`ValidationFinding`** — carries a rule name, `Severity` (`Error` /
  `Warning` / `Info`), a human-readable message, and an optional JSON-path-like
  `path` pointing at the offending field.

## Built-in rules

| Rule | Checks |
|---|---|
| `FilterSyntaxRule` | Balanced parentheses/brackets, non-empty filter, common `=` vs `==` typo |
| `PerformanceRule` | Excessive wildcard usage, `contains` combined with large field projections, missing field projection |
| `SchemaCompatibilityRule` | `schema_version` is set and is one of the versions this deployment supports |
| `TransformationRule` | Transform scripts don't reference disallowed host APIs (`os.`, `eval(`, `require(`, etc.) and aren't excessively large |
| `ResourceLimitRule` | `max_events_per_second`, `max_payload_bytes`, and `max_concurrent_deliveries` are set to sane, non-zero values |

## Usage

```rust
use soroban_pulse::subscription_validator::{ValidationEngine, SubscriptionConfig};

let engine = ValidationEngine::with_default_rules();
let config: SubscriptionConfig = serde_json::from_str(&raw_config_json)?;
let report = engine.validate(&config);

if report.has_errors() {
    for finding in report.errors() {
        eprintln!("[{}] {}", finding.rule, finding.message);
    }
    std::process::exit(1);
}
```

To validate every subscription config in a deployment manifest at once, use
`ValidationEngine::validate_all(&configs)`, which returns one `ValidationReport`
per input config.

## Extending

Custom checks (e.g. org-specific naming conventions) can be added without
modifying the built-in rule set:

```rust
struct NamingConventionRule;

impl ValidationRule for NamingConventionRule {
    fn name(&self) -> &'static str { "naming_convention" }
    fn validate(&self, config: &SubscriptionConfig) -> Vec<ValidationFinding> {
        if !config.name.starts_with("sub-") {
            vec![ValidationFinding::warning("naming_convention", "subscription names should be prefixed with 'sub-'")]
        } else {
            vec![]
        }
    }
}

let mut engine = ValidationEngine::new(); // empty, or start from with_default_rules()
engine.register(Box::new(NamingConventionRule));
```

## Testing

Run the validator's unit test suite with:

```
cargo test subscription_validator
```

Tests cover: valid configs producing no errors, empty/unbalanced filters,
unsupported schema versions, disallowed transform tokens, zero-value resource
limits, batch validation, and custom rule registration.
