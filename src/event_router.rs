//! Issue #815: multi-destination event streaming with guaranteed delivery.
//!
//! Sits in front of the per-platform publishers (`kinesis`, `pubsub`, `sqs`) and
//! decides which destinations an event goes to, batches it, retries it, and
//! parks it in a dead-letter queue when every attempt fails.
//!
//! The router is transport agnostic: destinations implement [`DestinationSink`],
//! so the same routing, batching and delivery-tracking logic covers every
//! platform and can be exercised in tests with an in-memory sink.

use crate::models::SorobanEvent;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

// === Destinations

/// Streaming platform a destination publishes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DestinationKind {
    #[serde(rename = "kinesis")]
    Kinesis,
    #[serde(rename = "pubsub")]
    PubSub,
    #[serde(rename = "sqs")]
    Sqs,
}

/// A batch of events tagged with the idempotency keys the sink must honour.
#[derive(Debug, Clone)]
pub struct DeliveryBatch {
    pub events: Vec<SorobanEvent>,
    /// Stable per-event keys so a retried batch is not double-published.
    pub idempotency_keys: Vec<String>,
}

/// A single publishing destination.
#[async_trait]
pub trait DestinationSink: Send + Sync {
    fn name(&self) -> &str;

    fn kind(&self) -> DestinationKind;

    /// Publish a batch. Implementations must be idempotent with respect to
    /// `batch.idempotency_keys` so retries after a partial failure are safe.
    async fn publish_batch(&self, batch: &DeliveryBatch) -> Result<(), String>;

    /// Cheap liveness probe used by the health checker.
    async fn health_check(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Sink that receives events no destination could accept.
#[async_trait]
pub trait DeadLetterSink: Send + Sync {
    async fn park(&self, destination: &str, batch: &DeliveryBatch, reason: &str) -> Result<(), String>;
}

// === Routing rules

/// Matcher deciding whether an event belongs on a destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RouteMatcher {
    /// Every event matches.
    All,
    ContractId(Vec<String>),
    EventType(Vec<String>),
    /// Only events at or above this ledger.
    MinLedger(u32),
    /// All inner matchers must match.
    AllOf(Vec<RouteMatcher>),
    /// Any inner matcher matches.
    Any(Vec<RouteMatcher>),
}

impl RouteMatcher {
    pub fn matches(&self, event: &SorobanEvent) -> bool {
        match self {
            RouteMatcher::All => true,
            RouteMatcher::ContractId(ids) => ids.iter().any(|id| *id == event.contract_id),
            RouteMatcher::EventType(types) => types.iter().any(|t| *t == event.event_type),
            RouteMatcher::MinLedger(min) => event.ledger as u64 >= *min as u64,
            RouteMatcher::AllOf(inner) => inner.iter().all(|m| m.matches(event)),
            RouteMatcher::Any(inner) => inner.iter().any(|m| m.matches(event)),
        }
    }
}

/// Binds a matcher to a destination, optionally overriding the topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub destination: String,
    pub matcher: RouteMatcher,
    /// Topic/stream override applied when this rule matches.
    pub topic: Option<String>,
    pub enabled: bool,
}

impl RoutingRule {
    pub fn new(destination: impl Into<String>, matcher: RouteMatcher) -> Self {
        Self {
            destination: destination.into(),
            matcher,
            topic: None,
            enabled: true,
        }
    }

    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = Some(topic.into());
        self
    }
}

/// Resolve which destinations an event should be published to.
pub fn resolve_destinations(rules: &[RoutingRule], event: &SorobanEvent) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for rule in rules.iter().filter(|r| r.enabled) {
        if rule.matcher.matches(event) && !seen.contains(&rule.destination) {
            seen.push(rule.destination.clone());
        }
    }
    seen
}

/// Topic a matched rule maps the event onto, falling back to the default topic.
pub fn resolve_topic<'a>(
    rules: &'a [RoutingRule],
    destination: &str,
    event: &SorobanEvent,
    default_topic: &'a str,
) -> &'a str {
    rules
        .iter()
        .filter(|r| r.enabled && r.destination == destination && r.matcher.matches(event))
        .find_map(|r| r.topic.as_deref())
        .unwrap_or(default_topic)
}

// === Rate limiting

/// Token bucket limiting publish rate per destination.
#[derive(Debug)]
pub struct RateLimiter {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_per_sec,
            last_refill: Instant::now(),
        }
    }

    /// Take `n` tokens, returning false when the destination is over budget.
    pub fn try_acquire(&mut self, n: f64) -> bool {
        let elapsed = self.last_refill.elapsed().as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_refill = Instant::now();

        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }
}

// === Delivery tracking

/// Terminal state of a batch delivery attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliveryStatus {
    #[serde(rename = "delivered")]
    Delivered,
    #[serde(rename = "retrying")]
    Retrying,
    #[serde(rename = "dead_lettered")]
    DeadLettered,
    #[serde(rename = "throttled")]
    Throttled,
}

/// Per-destination counters exposed to the delivery dashboard.
#[derive(Debug, Default, Serialize)]
pub struct DestinationMetrics {
    pub published: AtomicU64,
    pub failed: AtomicU64,
    pub retried: AtomicU64,
    pub dead_lettered: AtomicU64,
    pub throttled: AtomicU64,
    pub total_latency_ms: AtomicU64,
    pub batches: AtomicU64,
    pub healthy: AtomicU64,
}

impl DestinationMetrics {
    fn incr(counter: &AtomicU64, by: u64) {
        counter.fetch_add(by, Ordering::Relaxed);
    }

    /// Mean batch latency; 0 when nothing has been published yet.
    pub fn avg_latency_ms(&self) -> f64 {
        let batches = self.batches.load(Ordering::Relaxed);
        if batches == 0 {
            return 0.0;
        }
        self.total_latency_ms.load(Ordering::Relaxed) as f64 / batches as f64
    }

    pub fn snapshot(&self, destination: &str) -> DestinationMetricsSnapshot {
        DestinationMetricsSnapshot {
            destination: destination.to_string(),
            published: self.published.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            retried: self.retried.load(Ordering::Relaxed),
            dead_lettered: self.dead_lettered.load(Ordering::Relaxed),
            throttled: self.throttled.load(Ordering::Relaxed),
            avg_latency_ms: self.avg_latency_ms(),
            healthy: self.healthy.load(Ordering::Relaxed) == 1,
        }
    }
}

/// Serialisable view of a destination's counters for the dashboard endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct DestinationMetricsSnapshot {
    pub destination: String,
    pub published: u64,
    pub failed: u64,
    pub retried: u64,
    pub dead_lettered: u64,
    pub throttled: u64,
    pub avg_latency_ms: f64,
    pub healthy: bool,
}

// === Router

/// Router tuning knobs.
#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub max_batch_size: usize,
    pub max_retries: u32,
    pub retry_backoff_base_ms: u64,
    /// Publishes per second allowed per destination.
    pub rate_limit_per_sec: f64,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            max_retries: 3,
            retry_backoff_base_ms: 100,
            rate_limit_per_sec: 1000.0,
        }
    }
}

struct Destination {
    sink: Arc<dyn DestinationSink>,
    metrics: Arc<DestinationMetrics>,
    limiter: Mutex<RateLimiter>,
}

/// Routes events to every matching destination with retries and DLQ fallback.
pub struct EventRouter {
    destinations: HashMap<String, Destination>,
    rules: Vec<RoutingRule>,
    dead_letter: Option<Arc<dyn DeadLetterSink>>,
    config: RouterConfig,
}

impl EventRouter {
    pub fn new(config: RouterConfig) -> Self {
        Self {
            destinations: HashMap::new(),
            rules: Vec::new(),
            dead_letter: None,
            config,
        }
    }

    pub fn register(&mut self, sink: Arc<dyn DestinationSink>) -> &mut Self {
        let metrics = Arc::new(DestinationMetrics::default());
        metrics.healthy.store(1, Ordering::Relaxed);
        let limiter = Mutex::new(RateLimiter::new(
            self.config.rate_limit_per_sec,
            self.config.rate_limit_per_sec,
        ));
        info!(destination = %sink.name(), kind = ?sink.kind(), "Registered streaming destination");
        self.destinations.insert(
            sink.name().to_string(),
            Destination {
                sink,
                metrics,
                limiter,
            },
        );
        self
    }

    pub fn add_rule(&mut self, rule: RoutingRule) -> &mut Self {
        self.rules.push(rule);
        self
    }

    pub fn with_dead_letter(&mut self, dlq: Arc<dyn DeadLetterSink>) -> &mut Self {
        self.dead_letter = Some(dlq);
        self
    }

    pub fn rules(&self) -> &[RoutingRule] {
        &self.rules
    }

    /// Split events into batches no larger than the configured maximum.
    pub fn batch(&self, events: Vec<SorobanEvent>) -> Vec<DeliveryBatch> {
        events
            .chunks(self.config.max_batch_size.max(1))
            .map(|chunk| DeliveryBatch {
                events: chunk.to_vec(),
                idempotency_keys: chunk.iter().map(idempotency_key).collect(),
            })
            .collect()
    }

    /// Group events by the destinations their routing rules select.
    pub fn partition_by_destination(
        &self,
        events: &[SorobanEvent],
    ) -> HashMap<String, Vec<SorobanEvent>> {
        let mut grouped: HashMap<String, Vec<SorobanEvent>> = HashMap::new();
        for event in events {
            for destination in resolve_destinations(&self.rules, event) {
                grouped
                    .entry(destination)
                    .or_default()
                    .push(event.clone());
            }
        }
        grouped
    }

    /// Route events to every matching destination, batching and retrying per destination.
    ///
    /// A failure on one destination never blocks the others: each is reported
    /// separately so a single unhealthy platform cannot stall the pipeline.
    pub async fn route(&self, events: Vec<SorobanEvent>) -> HashMap<String, DeliveryStatus> {
        let mut outcomes = HashMap::new();

        for (destination, routed) in self.partition_by_destination(&events) {
            let Some(target) = self.destinations.get(&destination) else {
                warn!(destination = %destination, "Routing rule points at unknown destination");
                continue;
            };

            let mut status = DeliveryStatus::Delivered;
            for batch in self.batch(routed) {
                let batch_status = self.deliver(&destination, target, &batch).await;
                if batch_status != DeliveryStatus::Delivered {
                    status = batch_status;
                }
            }
            outcomes.insert(destination, status);
        }

        outcomes
    }

    async fn deliver(
        &self,
        name: &str,
        target: &Destination,
        batch: &DeliveryBatch,
    ) -> DeliveryStatus {
        let event_count = batch.events.len() as f64;
        if !target.limiter.lock().await.try_acquire(event_count) {
            DestinationMetrics::incr(&target.metrics.throttled, batch.events.len() as u64);
            warn!(destination = %name, events = batch.events.len(), "Destination rate limit exceeded");
            return DeliveryStatus::Throttled;
        }

        let mut last_error = String::new();
        for attempt in 0..=self.config.max_retries {
            let started = Instant::now();
            match target.sink.publish_batch(batch).await {
                Ok(()) => {
                    DestinationMetrics::incr(&target.metrics.published, batch.events.len() as u64);
                    DestinationMetrics::incr(
                        &target.metrics.total_latency_ms,
                        started.elapsed().as_millis() as u64,
                    );
                    DestinationMetrics::incr(&target.metrics.batches, 1);
                    target.metrics.healthy.store(1, Ordering::Relaxed);
                    return DeliveryStatus::Delivered;
                }
                Err(e) => {
                    last_error = e;
                    DestinationMetrics::incr(&target.metrics.failed, batch.events.len() as u64);
                    if attempt < self.config.max_retries {
                        DestinationMetrics::incr(&target.metrics.retried, 1);
                        let backoff =
                            self.config.retry_backoff_base_ms * 2_u64.pow(attempt);
                        warn!(
                            destination = %name,
                            attempt = attempt + 1,
                            backoff_ms = backoff,
                            error = %last_error,
                            "Publish failed, retrying"
                        );
                        tokio::time::sleep(Duration::from_millis(backoff)).await;
                    }
                }
            }
        }

        target.metrics.healthy.store(0, Ordering::Relaxed);
        error!(destination = %name, error = %last_error, "Publish exhausted retries");

        if let Some(dlq) = &self.dead_letter {
            match dlq.park(name, batch, &last_error).await {
                Ok(()) => {
                    DestinationMetrics::incr(
                        &target.metrics.dead_lettered,
                        batch.events.len() as u64,
                    );
                    return DeliveryStatus::DeadLettered;
                }
                Err(e) => error!(destination = %name, error = %e, "Dead-letter park failed"),
            }
        }

        DeliveryStatus::Retrying
    }

    /// Probe every destination and update its health flag.
    pub async fn health_checks(&self) -> Vec<(String, bool)> {
        let mut results = Vec::new();
        for (name, target) in &self.destinations {
            let healthy = match target.sink.health_check().await {
                Ok(()) => true,
                Err(e) => {
                    warn!(destination = %name, error = %e, "Destination health check failed");
                    false
                }
            };
            target
                .metrics
                .healthy
                .store(u64::from(healthy), Ordering::Relaxed);
            results.push((name.clone(), healthy));
        }
        results
    }

    /// Counters for every destination, backing the delivery dashboard.
    pub fn metrics_snapshot(&self) -> Vec<DestinationMetricsSnapshot> {
        let mut snapshots: Vec<DestinationMetricsSnapshot> = self
            .destinations
            .iter()
            .map(|(name, d)| d.metrics.snapshot(name))
            .collect();
        snapshots.sort_by(|a, b| a.destination.cmp(&b.destination));
        snapshots
    }

    /// Destinations whose delivery failure share exceeds `threshold`, for alerting.
    pub fn failing_destinations(&self, threshold: f64) -> Vec<DestinationMetricsSnapshot> {
        self.metrics_snapshot()
            .into_iter()
            .filter(|s| {
                let total = s.published + s.failed;
                total > 0 && (s.failed as f64 / total as f64) > threshold
            })
            .collect()
    }
}

/// Stable key identifying an event across retries and destinations.
pub fn idempotency_key(event: &SorobanEvent) -> String {
    format!("{}:{}:{}", event.tx_hash, event.contract_id, event.ledger)
}
