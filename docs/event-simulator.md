# Soroban Event Simulator

A tool for generating simulated Soroban contract events for testing —
without requiring a live network connection. Implemented in
[`src/event_simulator.rs`](../src/event_simulator.rs).

## Why

Testing subscription filters, transformation logic, and system behavior
under load usually requires real ledger events, which are slow to obtain
and non-deterministic. The simulator generates realistic, reproducible
events on demand so filters and pipelines can be exercised in unit and
integration tests, and so load characteristics can be explored without
touching a live network.

## Core types

- **`EventPattern`** — a template describing the shape of events to
  generate: contract id, event type, topics, and a JSON data payload
  template.
- **`EventFactory`** — generates `SimulatedEvent`s from patterns. Seeded
  with a `u64` for deterministic, reproducible output (uses an internal
  xorshift PRNG — no external `rand` dependency).
  - `generate(pattern)` — one event
  - `generate_bulk(pattern, count)` — many events from one pattern
  - `generate_mixed(patterns, count)` — cycles through several patterns to
    simulate a mixed workload
  - `generate_sequence(patterns)` — one event per pattern, in order, for
    modeling event lifecycles (e.g. `submitted -> executed -> settled`)
  - `generate_load(pattern, count, avg_gap_ms)` — events paired with
    randomized delay jitter, for simulating realistic traffic timing
- **`FilterRule`** — a small boolean expression tree (`ContractIdEquals`,
  `EventTypeEquals`, `HasTopic`, `DataFieldEquals`, `And`, `Or`, `Not`) for
  testing filter logic against generated events.
- **`FilterTester`** — runs a `FilterRule` against a batch of events and
  reports match count/rate — useful for sanity-checking a subscription
  filter's selectivity before deployment.
- **`EventReplay`** — replays a generated (or recorded) event sequence,
  either all at once via a callback or one at a time via `step()`, for
  feeding events into a pipeline under test.

## Usage

```rust
use soroban_pulse::event_simulator::{EventFactory, EventPattern, FilterRule, FilterTester};
use serde_json::json;

let pattern = EventPattern::new("CCONTRACT123", "payment")
    .with_topics(vec!["payments".into()])
    .with_data(json!({ "amount": 100 }));

let mut factory = EventFactory::new(42); // seeded for reproducibility
let events = factory.generate_bulk(&pattern, 1_000);

let rule = FilterRule::DataFieldEquals("amount".into(), json!(100));
let result = FilterTester::test(&rule, &events);
println!("matched {}/{} ({:.1}%)", result.matched, result.total_events, result.match_rate * 100.0);
```

### Load generation

```rust
let load = factory.generate_load(&pattern, 10_000, /* avg_gap_ms */ 5);
for (event, delay_ms) in load {
    // feed into a pipeline with `delay_ms` between deliveries
}
```

### Replay

```rust
use soroban_pulse::event_simulator::EventReplay;

let mut replay = EventReplay::new(events);
replay.replay_all(|event| {
    // hand each event to the subscription pipeline under test
});
```

## Testing

```
cargo test event_simulator
```

Covers: unique id generation, deterministic output given a seed, bulk and
mixed generation, sequence ordering, load jitter bounds, filter rule
matching (including composed `And`/`Or`/`Not` expressions and data-field
matching), and full/step-wise replay.
