# Contract Event Simulation (Issue #590)

## Overview

Contract Event Simulation provides realistic contract event fixtures for testing. It enables creating representative test data without requiring live contract execution or network access.

## Features

### Event Generator

The event simulator generates realistic Soroban contract events:

- **Transfer Events** - Token transfer simulation with realistic XDR encoding
- **Invoke Events** - Contract function call simulation
- **Sequential Ledgers** - Events with incrementing ledger numbers
- **Realistic Timestamps** - RFC3339 formatted timestamps

### XDR Fixture Generation

Generates valid XDR (External Data Representation) fixtures for:

- Event topics (base64-encoded)
- Event data (base64-encoded parameters)
- Contract invocation data
- Transfer event data

### Contract Scenario Templates

Pre-built scenario templates for common testing cases:

- **Simple Transfer** - Single token transfer
- **Multi-hop Transfer** - Series of sequential transfers
- **Contract Interaction** - Invoke followed by state change
- **High Volume Scenario** - 1000+ events in sequence

## Using the Event Simulator

### Basic Event Generation

```rust
use soroban_pulse::event_fixtures::EventSimulator;

let mut simulator = EventSimulator::new();

// Generate transfer event
let transfer_event = simulator.generate_transfer_event();

// Generate invoke event
let invoke_event = simulator.generate_invoke_event();

// Generate XDR fixture
let xdr_fixture = simulator.generate_xdr_fixture();
```

### Generating Scenarios

```rust
// Generate 10-event scenario
let scenario = simulator.generate_contract_scenario(10);

// Verify all events have valid structure
for event in scenario {
    assert!(event["id"].is_string());
    assert!(event["ledger"].is_number());
    assert!(event["event_data"]["topics"].is_array());
}
```

## Event Structure

### Transfer Event Example

```json
{
  "id": "evt_550e8400-e29b-41d4-a716-446655440000",
  "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB5NQ",
  "event_type": "contract",
  "ledger": 1001,
  "ledger_hash": "00000000000000000000000000000000000000000000000000000000000003e9",
  "sequence": 10,
  "timestamp": "2024-01-01T12:00:01Z",
  "event_data": {
    "topics": ["AAAADwAAAAVUUkFOU0ZFUg=="],
    "data": [
      "AAAAEQAAAAAAAAAAAOwwYy4S5sNkfP7bJmDVwQHvQJk/8nXvWCvvFevt/qE=",
      "AAAAEQAAAAAAAAAAAOwwYy4S5sNkfP7bJmDVwQHvQJk/8nXvWCvvFevt/qE=",
      "AAAACgAAAAAAB9QA"
    ]
  },
  "created_at": "2024-01-01T12:00:01Z"
}
```

### Event Topics

Base64-encoded event type indicators:

- `AAAADwAAAAVUUkFOU0ZFUg==` - TRANSFER event
- `AAAADwAAAAZpbnZva2Vk` - INVOKED event
- `AAAADwAAAAdhcHByb3ZlZA==` - APPROVE event

## Data Factories

### Contract Factory

```rust
use soroban_pulse::event_fixtures::ContractFactory;

let mut factory = ContractFactory::new();

// Create contract
let contract = factory.create_contract();

// Create subscription
let subscription = factory.create_subscription(
    contract["contract_id"].as_str().unwrap().to_string()
);

// Create notification
let notification = factory.create_notification(event);
```

### Factory-Generated Objects

**Contract**:

```json
{
  "id": "contract_1",
  "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB5NQ",
  "name": "TestContract1",
  "network": "testnet",
  "created_at": "2024-01-01T12:00:00Z",
  "event_count": 0,
  "last_event_ledger": 0
}
```

**Subscription**:

```json
{
  "id": "sub_550e8400-e29b-41d4-a716-446655440000",
  "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB5NQ",
  "event_types": ["contract"],
  "webhook_url": "https://webhook.example.com/events",
  "filter": {
    "topics": ["transfer", "approve"]
  },
  "status": "active",
  "created_at": "2024-01-01T12:00:00Z"
}
```

## Running Event Simulation Tests

```bash
# Run all event simulation tests
cargo test event_simulation

# Generate transfer event
cargo test generate_realistic_transfer_event -- --nocapture

# Generate contract scenario
cargo test generate_contract_scenario -- --nocapture

# Run XDR fixture tests
cargo test generate_xdr_fixture -- --nocapture
```

## Common Testing Scenarios

### Testing Event Filters

```rust
let mut simulator = EventSimulator::new();
let scenario = simulator.generate_contract_scenario(100);

// Filter by event type
let transfer_events: Vec<_> = scenario
    .iter()
    .filter(|e| e["event_data"]["topics"][0]
        .as_str()
        .unwrap()
        .contains("VFJBTVNGR"))
    .collect();

assert!(!transfer_events.is_empty());
```

### Testing Webhook Delivery

```rust
let mut simulator = EventSimulator::new();
let event = simulator.generate_transfer_event();
let mut factory = ContractFactory::new();
let subscription = factory.create_subscription(
    event["contract_id"].as_str().unwrap().to_string()
);

// Send webhook with simulated event
send_webhook(&subscription, &event).await?;
```

### Testing Event Deduplication

```rust
let mut simulator = EventSimulator::new();
let event1 = simulator.generate_transfer_event();

// Duplicate event with same data
let mut event2 = event1.clone();
event2["id"] = json!("evt_123_dup");

// Test deduplication logic
assert!(should_deduplicate(&event1, &event2));
```

## XDR Encoding Reference

### Topics Encoding

Topics are event type indicators, base64-encoded:

| Event    | Encoding                 | Decoded  |
| -------- | ------------------------ | -------- |
| TRANSFER | AAAADwAAAAVUUkFOU0ZFUg== | transfer |
| APPROVE  | AAAADwAAAAdhcHByb3ZlZA== | approved |
| INVOKE   | AAAADwAAAAZpbnZva2Vk     | invoked  |

### Data Encoding

Event data parameters are base64-encoded contract values:

- Account addresses: `AAAAEQAAAAAAAAAAAA...` (32 bytes)
- Amounts: `AAAACgAAAAAAB9QA` (i128 value)
- Booleans: `AAAAAQ==` (1) or `AAAAAA==` (0)

## Integration with Testing Pipeline

### Unit Tests

Use event simulation for unit test fixtures:

```rust
#[test]
fn test_filter_logic() {
    let mut simulator = EventSimulator::new();
    let event = simulator.generate_transfer_event();

    // Test filter against simulated event
    assert!(filter_matches(&event, &contract_filter));
}
```

### Integration Tests

Use scenarios for integration test workflows:

```rust
#[sqlx::test]
async fn test_webhook_delivery_pipeline(pool: PgPool) {
    let mut simulator = EventSimulator::new();
    let scenario = simulator.generate_contract_scenario(10);

    // Process entire scenario
    for event in scenario {
        process_event(&pool, event).await.unwrap();
    }
}
```

### Performance Tests

Use high-volume scenarios for performance testing:

```rust
#[test]
fn bench_event_filtering() {
    let mut simulator = EventSimulator::new();
    let scenario = simulator.generate_contract_scenario(10000);

    let start = Instant::now();
    for event in scenario {
        apply_filter(&event);
    }
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(1));
}
```

## Best Practices

1. **Use Realistic Data** - Base event structure on actual Soroban events
2. **Vary Scenarios** - Test with different event types and combinations
3. **Include Edge Cases** - Test with edge case values (0, max, empty)
4. **Sequential Ledgers** - Ensure ledger numbers increment properly
5. **Valid Timestamps** - Use valid RFC3339 format

## Related Issues

- #591 - Performance Regression Testing
- #556 - Contract Testing Framework
- #267 - XDR Validation
