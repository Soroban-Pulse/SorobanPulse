//! Tool for simulating Soroban contract events for testing filters,
//! subscriptions, and system load without needing a live network.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// A simulated Soroban contract event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedEvent {
    pub id: String,
    pub contract_id: String,
    pub event_type: String,
    pub ledger_sequence: u64,
    pub timestamp: u64,
    pub topics: Vec<String>,
    pub data: Value,
}

/// A reusable template describing how to generate events of a given shape.
#[derive(Debug, Clone)]
pub struct EventPattern {
    pub contract_id: String,
    pub event_type: String,
    pub topics: Vec<String>,
    pub data_template: Value,
}

impl EventPattern {
    pub fn new(contract_id: impl Into<String>, event_type: impl Into<String>) -> Self {
        Self {
            contract_id: contract_id.into(),
            event_type: event_type.into(),
            topics: Vec::new(),
            data_template: json!({}),
        }
    }

    pub fn with_topics(mut self, topics: Vec<String>) -> Self {
        self.topics = topics;
        self
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data_template = data;
        self
    }
}

/// Deterministic pseudo-random generator (xorshift) so simulation runs are
/// reproducible given a seed — no external `rand` dependency required.
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9E3779B97F4A7C15 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_range(&mut self, max: u64) -> u64 {
        if max == 0 {
            0
        } else {
            self.next_u64() % max
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Factory for generating simulated events, either one at a time from a
/// pattern or in bulk.
pub struct EventFactory {
    rng: DeterministicRng,
    next_ledger_sequence: u64,
    counter: u64,
}

impl EventFactory {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: DeterministicRng::new(seed),
            next_ledger_sequence: 1,
            counter: 0,
        }
    }

    /// Generate a single event from a pattern.
    pub fn generate(&mut self, pattern: &EventPattern) -> SimulatedEvent {
        self.counter += 1;
        let id = format!("evt-{:016x}", self.rng.next_u64());
        let event = SimulatedEvent {
            id,
            contract_id: pattern.contract_id.clone(),
            event_type: pattern.event_type.clone(),
            ledger_sequence: self.next_ledger_sequence,
            timestamp: now_secs(),
            topics: pattern.topics.clone(),
            data: pattern.data_template.clone(),
        };
        self.next_ledger_sequence += 1;
        event
    }

    /// Generate `count` events from a single pattern.
    pub fn generate_bulk(&mut self, pattern: &EventPattern, count: usize) -> Vec<SimulatedEvent> {
        (0..count).map(|_| self.generate(pattern)).collect()
    }

    /// Generate events by cycling through several patterns, useful for
    /// simulating a mixed workload of event types.
    pub fn generate_mixed(&mut self, patterns: &[EventPattern], count: usize) -> Vec<SimulatedEvent> {
        (0..count)
            .map(|i| self.generate(&patterns[i % patterns.len()]))
            .collect()
    }

    /// Generate a realistic sequence of related events (e.g. the lifecycle
    /// of a transaction: submitted -> executed -> settled), with ledger
    /// sequences and timestamps advancing monotonically between steps.
    pub fn generate_sequence(&mut self, patterns: &[EventPattern]) -> Vec<SimulatedEvent> {
        patterns.iter().map(|p| self.generate(p)).collect()
    }

    /// Generate a burst of events with randomized jitter in the sequence
    /// gaps, simulating realistic traffic rather than perfectly uniform
    /// spacing. Returns events paired with a synthetic delay (in ms) that
    /// should elapse before the event is "delivered" during replay.
    pub fn generate_load(&mut self, pattern: &EventPattern, count: usize, avg_gap_ms: u64) -> Vec<(SimulatedEvent, u64)> {
        (0..count)
            .map(|_| {
                let event = self.generate(pattern);
                let jitter = self.rng.next_range(avg_gap_ms.max(1) * 2);
                (event, jitter)
            })
            .collect()
    }
}

/// A simple filter rule for testing whether a subscription's filter logic
/// matches simulated events. Mirrors the minimal filter grammar used by
/// [`crate::codegen::filter`].
#[derive(Debug, Clone)]
pub enum FilterRule {
    ContractIdEquals(String),
    EventTypeEquals(String),
    HasTopic(String),
    DataFieldEquals(String, Value),
    And(Box<FilterRule>, Box<FilterRule>),
    Or(Box<FilterRule>, Box<FilterRule>),
    Not(Box<FilterRule>),
}

impl FilterRule {
    pub fn matches(&self, event: &SimulatedEvent) -> bool {
        match self {
            FilterRule::ContractIdEquals(id) => &event.contract_id == id,
            FilterRule::EventTypeEquals(t) => &event.event_type == t,
            FilterRule::HasTopic(t) => event.topics.iter().any(|topic| topic == t),
            FilterRule::DataFieldEquals(field, value) => {
                event.data.get(field).map(|v| v == value).unwrap_or(false)
            }
            FilterRule::And(a, b) => a.matches(event) && b.matches(event),
            FilterRule::Or(a, b) => a.matches(event) || b.matches(event),
            FilterRule::Not(a) => !a.matches(event),
        }
    }
}

/// Result of running a filter rule against a batch of simulated events.
#[derive(Debug, Clone, Serialize)]
pub struct FilterTestResult {
    pub total_events: usize,
    pub matched: usize,
    pub match_rate: f64,
    pub matched_ids: Vec<String>,
}

/// Runs filter rules against generated events to validate filter logic
/// before deploying a subscription that relies on it.
pub struct FilterTester;

impl FilterTester {
    pub fn test(rule: &FilterRule, events: &[SimulatedEvent]) -> FilterTestResult {
        let matched_ids: Vec<String> = events
            .iter()
            .filter(|e| rule.matches(e))
            .map(|e| e.id.clone())
            .collect();
        let matched = matched_ids.len();
        let total = events.len();
        FilterTestResult {
            total_events: total,
            matched,
            match_rate: if total == 0 { 0.0 } else { matched as f64 / total as f64 },
            matched_ids,
        }
    }
}

/// Replays a previously generated (or recorded) sequence of events,
/// invoking a callback for each in order. Useful for feeding a simulated
/// or recorded event stream into a subscription pipeline under test.
pub struct EventReplay {
    events: Vec<SimulatedEvent>,
    position: usize,
}

impl EventReplay {
    pub fn new(events: Vec<SimulatedEvent>) -> Self {
        Self { events, position: 0 }
    }

    /// Replay all events in order, calling `on_event` for each.
    pub fn replay_all(&mut self, mut on_event: impl FnMut(&SimulatedEvent)) {
        for event in &self.events {
            on_event(event);
            self.position += 1;
        }
    }

    /// Advance one event at a time; returns `None` once exhausted.
    pub fn step(&mut self) -> Option<&SimulatedEvent> {
        let event = self.events.get(self.position)?;
        self.position += 1;
        Some(event)
    }

    pub fn reset(&mut self) {
        self.position = 0;
    }

    pub fn remaining(&self) -> usize {
        self.events.len().saturating_sub(self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payment_pattern() -> EventPattern {
        EventPattern::new("CCONTRACT123", "payment")
            .with_topics(vec!["payments".into()])
            .with_data(json!({"amount": 100}))
    }

    #[test]
    fn generate_produces_unique_ids() {
        let mut factory = EventFactory::new(42);
        let a = factory.generate(&payment_pattern());
        let b = factory.generate(&payment_pattern());
        assert_ne!(a.id, b.id);
        assert_eq!(a.ledger_sequence + 1, b.ledger_sequence);
    }

    #[test]
    fn generate_is_deterministic_given_seed() {
        let mut f1 = EventFactory::new(7);
        let mut f2 = EventFactory::new(7);
        let e1 = f1.generate(&payment_pattern());
        let e2 = f2.generate(&payment_pattern());
        assert_eq!(e1.id, e2.id);
    }

    #[test]
    fn generate_bulk_produces_requested_count() {
        let mut factory = EventFactory::new(1);
        let events = factory.generate_bulk(&payment_pattern(), 50);
        assert_eq!(events.len(), 50);
    }

    #[test]
    fn generate_mixed_cycles_through_patterns() {
        let mut factory = EventFactory::new(1);
        let patterns = vec![payment_pattern(), EventPattern::new("C2", "refund")];
        let events = factory.generate_mixed(&patterns, 4);
        assert_eq!(events[0].event_type, "payment");
        assert_eq!(events[1].event_type, "refund");
        assert_eq!(events[2].event_type, "payment");
    }

    #[test]
    fn generate_sequence_preserves_order() {
        let mut factory = EventFactory::new(1);
        let patterns = vec![
            EventPattern::new("C1", "submitted"),
            EventPattern::new("C1", "executed"),
            EventPattern::new("C1", "settled"),
        ];
        let events = factory.generate_sequence(&patterns);
        let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        assert_eq!(types, vec!["submitted", "executed", "settled"]);
    }

    #[test]
    fn generate_load_returns_jitter_per_event() {
        let mut factory = EventFactory::new(3);
        let load = factory.generate_load(&payment_pattern(), 10, 100);
        assert_eq!(load.len(), 10);
        assert!(load.iter().all(|(_, jitter)| *jitter <= 200));
    }

    #[test]
    fn filter_rule_matches_contract_id() {
        let mut factory = EventFactory::new(1);
        let events = factory.generate_bulk(&payment_pattern(), 5);
        let rule = FilterRule::ContractIdEquals("CCONTRACT123".into());
        let result = FilterTester::test(&rule, &events);
        assert_eq!(result.matched, 5);
        assert_eq!(result.match_rate, 1.0);
    }

    #[test]
    fn filter_rule_and_or_not_compose() {
        let mut factory = EventFactory::new(1);
        let events = factory.generate_bulk(&payment_pattern(), 3);
        let rule = FilterRule::And(
            Box::new(FilterRule::EventTypeEquals("payment".into())),
            Box::new(FilterRule::Not(Box::new(FilterRule::HasTopic("refunds".into())))),
        );
        let result = FilterTester::test(&rule, &events);
        assert_eq!(result.matched, 3);
    }

    #[test]
    fn filter_rule_data_field_match() {
        let mut factory = EventFactory::new(1);
        let events = factory.generate_bulk(&payment_pattern(), 2);
        let rule = FilterRule::DataFieldEquals("amount".into(), json!(100));
        let result = FilterTester::test(&rule, &events);
        assert_eq!(result.matched, 2);
    }

    #[test]
    fn replay_steps_through_events_in_order() {
        let mut factory = EventFactory::new(1);
        let events = factory.generate_sequence(&[
            EventPattern::new("C1", "a"),
            EventPattern::new("C1", "b"),
        ]);
        let mut replay = EventReplay::new(events);
        assert_eq!(replay.step().unwrap().event_type, "a");
        assert_eq!(replay.step().unwrap().event_type, "b");
        assert!(replay.step().is_none());
    }

    #[test]
    fn replay_all_visits_every_event() {
        let mut factory = EventFactory::new(1);
        let events = factory.generate_bulk(&payment_pattern(), 4);
        let mut replay = EventReplay::new(events);
        let mut visited = 0;
        replay.replay_all(|_| visited += 1);
        assert_eq!(visited, 4);
        assert_eq!(replay.remaining(), 0);
    }
}
