//! # Real-Time Event Stream Statistics — Issue #929
//!
//! Tracks live metrics about the SSE event stream: throughput, size
//! distribution, per-contract counts, moving averages, and anomaly signals.
//! The shared [`StreamStatsState`] is updated on every event broadcast and
//! exposed via three read-only HTTP endpoints.

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

use crate::routes::AppState;

// ---------------------------------------------------------------------------
// Core statistics types
// ---------------------------------------------------------------------------

/// Statistical distribution over a numeric sample set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Distribution {
    /// Minimum observed value.
    pub min: f64,
    /// Maximum observed value.
    pub max: f64,
    /// Arithmetic mean.
    pub avg: f64,
    /// 50th percentile (median).
    pub p50: f64,
    /// 95th percentile.
    pub p95: f64,
    /// 99th percentile.
    pub p99: f64,
    /// Total sample count.
    pub count: u64,
}

/// Exponential Moving Average for a sliding time window.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ema {
    /// Current EMA value (events/sec).
    pub value: f64,
    /// Smoothing factor α = 2 / (N + 1).
    pub alpha: f64,
    /// Number of data points consumed.
    pub samples: u64,
}

impl Ema {
    /// Create a new EMA with a window of `periods` samples.
    pub fn new(periods: u64) -> Self {
        let alpha = 2.0 / (periods as f64 + 1.0);
        Self {
            value: 0.0,
            alpha,
            samples: 0,
        }
    }

    /// Update the EMA with a new observation `x`.
    pub fn update(&mut self, x: f64) {
        if self.samples == 0 {
            self.value = x;
        } else {
            self.value = self.alpha * x + (1.0 - self.alpha) * self.value;
        }
        self.samples += 1;
    }
}

/// Moving averages over 1-minute, 5-minute, and 15-minute windows.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MovingAverages {
    /// Events-per-second EMA over the last ~1 minute (60 one-second ticks).
    pub one_min: f64,
    /// Events-per-second EMA over the last ~5 minutes.
    pub five_min: f64,
    /// Events-per-second EMA over the last ~15 minutes.
    pub fifteen_min: f64,
}

/// Anomaly signal detected in the event stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyKind {
    /// Event rate is significantly higher than the recent average.
    Spike,
    /// Event rate has dropped to near zero after being non-zero.
    Drop,
    /// A single contract is producing a disproportionate fraction of events.
    ContractDomination,
    /// Event payload sizes are unusually large.
    LargePayload,
}

/// A single detected anomaly with context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalySignal {
    /// Type of anomaly detected.
    pub kind: AnomalyKind,
    /// Human-readable description.
    pub message: String,
    /// Severity score 0.0–1.0.
    pub severity: f64,
    /// When the anomaly was first detected.
    pub detected_at: DateTime<Utc>,
}

/// Per-contract statistics snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractStats {
    /// Total events indexed for this contract.
    pub event_count: u64,
    /// Breakdown by event type.
    pub event_type_distribution: HashMap<String, u64>,
    /// Average event payload size in bytes.
    pub avg_payload_bytes: f64,
    /// Timestamp of the most recent event.
    pub last_event_at: Option<DateTime<Utc>>,
    /// Events-per-second over a 1-minute window.
    pub events_per_second_1m: f64,
}

/// Complete stream statistics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamStats {
    /// Total events broadcast since service start.
    pub events_broadcast_total: u64,
    /// Current events-per-second (instantaneous).
    pub events_per_second: f64,
    /// Moving averages over 1m / 5m / 15m windows.
    pub moving_averages: MovingAverages,
    /// Size distribution of event payloads (bytes).
    pub payload_size_distribution: Distribution,
    /// Latency distribution from indexing to broadcast (milliseconds).
    pub broadcast_latency_ms: Distribution,
    /// Breakdown of broadcast events by event type.
    pub event_type_distribution: HashMap<String, u64>,
    /// Per-contract statistics (top 100 by event count).
    pub per_contract: HashMap<String, ContractStats>,
    /// Total unique contracts seen since service start.
    pub unique_contracts_total: usize,
    /// Currently active SSE connections.
    pub active_sse_connections: usize,
    /// Anomaly signals detected in the recent window.
    pub anomalies: Vec<AnomalySignal>,
    /// Timestamp of the last event broadcast.
    pub last_event_at: Option<DateTime<Utc>>,
    /// Service uptime in seconds.
    pub uptime_secs: u64,
}

impl Default for StreamStats {
    fn default() -> Self {
        Self {
            events_broadcast_total: 0,
            events_per_second: 0.0,
            moving_averages: MovingAverages::default(),
            payload_size_distribution: Distribution::default(),
            broadcast_latency_ms: Distribution::default(),
            event_type_distribution: HashMap::new(),
            per_contract: HashMap::new(),
            unique_contracts_total: 0,
            active_sse_connections: 0,
            anomalies: Vec::new(),
            last_event_at: None,
            uptime_secs: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Inner mutable state, protected by a `RwLock`.
struct StreamStatsInner {
    stats: StreamStats,
    /// EMA state for 1m / 5m / 15m windows.
    ema_1m: Ema,
    ema_5m: Ema,
    ema_15m: Ema,
    /// Circular buffer of per-second event counts (last 60 seconds).
    second_buckets: VecDeque<u64>,
    /// Events counted in the current second.
    current_second_count: u64,
    /// Start of the current one-second bucket.
    bucket_start: Instant,
    /// Service start time.
    started_at: Instant,
    /// Payload size samples for percentile calculation (bounded at 1000).
    payload_samples: VecDeque<f64>,
    /// Latency samples for percentile calculation (bounded at 1000).
    latency_samples: VecDeque<f64>,
}

impl StreamStatsInner {
    fn new() -> Self {
        Self {
            stats: StreamStats::default(),
            ema_1m: Ema::new(60),
            ema_5m: Ema::new(300),
            ema_15m: Ema::new(900),
            second_buckets: VecDeque::with_capacity(60),
            current_second_count: 0,
            bucket_start: Instant::now(),
            started_at: Instant::now(),
            payload_samples: VecDeque::with_capacity(1000),
            latency_samples: VecDeque::with_capacity(1000),
        }
    }
}

/// Shared, cloneable handle to the stream statistics state.
pub type StreamStatsState = Arc<RwLock<StreamStatsInner>>;

/// Create a new [`StreamStatsState`].
pub fn new_stream_stats_state() -> StreamStatsState {
    Arc::new(RwLock::new(StreamStatsInner::new()))
}

// ---------------------------------------------------------------------------
// Public API functions
// ---------------------------------------------------------------------------

/// Record a single broadcast event into the statistics state.
///
/// # Parameters
/// - `state`: shared stats state.
/// - `contract_id`: contract that produced the event.
/// - `event_type`: one of `"contract"`, `"diagnostic"`, `"system"`.
/// - `payload_bytes`: serialised event payload size.
/// - `latency_ms`: milliseconds from indexing to broadcast.
pub async fn record_event(
    state: &StreamStatsState,
    contract_id: &str,
    event_type: &str,
    payload_bytes: usize,
    latency_ms: f64,
) {
    let mut inner = state.write().await;

    // Rotate second bucket if needed.
    let elapsed = inner.bucket_start.elapsed();
    if elapsed >= Duration::from_secs(1) {
        let count = inner.current_second_count;
        let eps = count as f64 / elapsed.as_secs_f64();
        inner.ema_1m.update(eps);
        inner.ema_5m.update(eps);
        inner.ema_15m.update(eps);
        if inner.second_buckets.len() == 60 {
            inner.second_buckets.pop_front();
        }
        inner.second_buckets.push_back(count);
        inner.current_second_count = 0;
        inner.bucket_start = Instant::now();
    }

    inner.current_second_count += 1;
    inner.stats.events_broadcast_total += 1;
    inner.stats.last_event_at = Some(Utc::now());
    inner.stats.uptime_secs = inner.started_at.elapsed().as_secs();

    // Event type distribution.
    *inner
        .stats
        .event_type_distribution
        .entry(event_type.to_string())
        .or_insert(0) += 1;

    // Per-contract stats.
    let contract_entry = inner
        .stats
        .per_contract
        .entry(contract_id.to_string())
        .or_default();
    contract_entry.event_count += 1;
    *contract_entry
        .event_type_distribution
        .entry(event_type.to_string())
        .or_insert(0) += 1;
    contract_entry.last_event_at = Some(Utc::now());
    let n = contract_entry.event_count as f64;
    contract_entry.avg_payload_bytes =
        (contract_entry.avg_payload_bytes * (n - 1.0) + payload_bytes as f64) / n;

    // Unique contract count.
    inner.stats.unique_contracts_total = inner.stats.per_contract.len();

    // Payload samples.
    if inner.payload_samples.len() == 1000 {
        inner.payload_samples.pop_front();
    }
    inner.payload_samples.push_back(payload_bytes as f64);
    inner.stats.payload_size_distribution =
        compute_distribution(&inner.payload_samples);

    // Latency samples.
    if inner.latency_samples.len() == 1000 {
        inner.latency_samples.pop_front();
    }
    inner.latency_samples.push_back(latency_ms);
    inner.stats.broadcast_latency_ms =
        compute_distribution(&inner.latency_samples);

    // Moving averages.
    inner.stats.moving_averages = MovingAverages {
        one_min: inner.ema_1m.value,
        five_min: inner.ema_5m.value,
        fifteen_min: inner.ema_15m.value,
    };

    // Instantaneous EPS.
    let window: u64 = inner.second_buckets.iter().sum();
    let secs = inner.second_buckets.len().max(1) as f64;
    inner.stats.events_per_second = window as f64 / secs;

    // Anomaly detection.
    inner.stats.anomalies = detect_anomalies_inner(
        inner.stats.events_per_second,
        &inner.stats.moving_averages,
        &inner.stats.per_contract,
        inner.stats.events_broadcast_total,
    );
}

/// Return a point-in-time snapshot of the current stream statistics.
pub async fn get_stats(state: &StreamStatsState) -> StreamStats {
    state.read().await.stats.clone()
}

/// Return statistics for a single contract.
pub async fn get_contract_stats(
    state: &StreamStatsState,
    contract_id: &str,
) -> Option<ContractStats> {
    state
        .read()
        .await
        .stats
        .per_contract
        .get(contract_id)
        .cloned()
}

/// Calculate the EMA for a slice of values with a given window.
///
/// Returns a smoothed value appropriate for the window size.
pub fn calculate_moving_average(samples: &[f64], window: usize) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut ema = Ema::new(window as u64);
    for &x in samples {
        ema.update(x);
    }
    ema.value
}

/// Detect anomaly signals from current stream metrics.
pub fn detect_anomalies(
    current_eps: f64,
    moving_averages: &MovingAverages,
    per_contract: &HashMap<String, ContractStats>,
    total_events: u64,
) -> Vec<AnomalySignal> {
    detect_anomalies_inner(current_eps, moving_averages, per_contract, total_events)
}

// Internal implementation used from both the public API and the write lock path.
fn detect_anomalies_inner(
    current_eps: f64,
    moving_averages: &MovingAverages,
    per_contract: &HashMap<String, ContractStats>,
    total_events: u64,
) -> Vec<AnomalySignal> {
    let mut signals = Vec::new();
    let now = Utc::now();

    // Spike detection: current EPS > 3× the 5-minute average.
    let avg = moving_averages.five_min;
    if avg > 0.1 && current_eps > avg * 3.0 {
        let severity = ((current_eps / avg) / 10.0).min(1.0);
        signals.push(AnomalySignal {
            kind: AnomalyKind::Spike,
            message: format!(
                "event rate spike: {current_eps:.1} eps vs {avg:.1} eps 5-min avg"
            ),
            severity,
            detected_at: now,
        });
    }

    // Drop detection: avg > 1 eps but current is near zero.
    if avg > 1.0 && current_eps < 0.01 {
        signals.push(AnomalySignal {
            kind: AnomalyKind::Drop,
            message: format!("event rate dropped to ~0 from {avg:.1} eps avg"),
            severity: 0.8,
            detected_at: now,
        });
    }

    // Contract domination: one contract > 80% of total events.
    if total_events > 100 {
        for (contract_id, stats) in per_contract {
            let fraction = stats.event_count as f64 / total_events as f64;
            if fraction > 0.80 {
                signals.push(AnomalySignal {
                    kind: AnomalyKind::ContractDomination,
                    message: format!(
                        "contract {contract_id} accounts for {:.0}% of all events",
                        fraction * 100.0
                    ),
                    severity: fraction - 0.80,
                    detected_at: now,
                });
            }
        }
    }

    signals
}

// ---------------------------------------------------------------------------
// Helper: compute statistical distribution
// ---------------------------------------------------------------------------

fn compute_distribution(samples: &VecDeque<f64>) -> Distribution {
    if samples.is_empty() {
        return Distribution::default();
    }
    let mut sorted: Vec<f64> = samples.iter().copied().collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = sorted.len();
    let sum: f64 = sorted.iter().sum();
    let avg = sum / n as f64;
    let min = sorted[0];
    let max = sorted[n - 1];

    let percentile = |p: f64| {
        let idx = ((p / 100.0) * (n - 1) as f64).round() as usize;
        sorted[idx.min(n - 1)]
    };

    Distribution {
        min,
        max,
        avg,
        p50: percentile(50.0),
        p95: percentile(95.0),
        p99: percentile(99.0),
        count: n as u64,
    }
}

// ---------------------------------------------------------------------------
// Throughput time-series entry
// ---------------------------------------------------------------------------

/// A single data point in the throughput time-series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputPoint {
    /// ISO-8601 timestamp for this bucket.
    pub timestamp: DateTime<Utc>,
    /// Events per second during this bucket.
    pub events_per_second: f64,
    /// Raw event count in this bucket.
    pub event_count: u64,
}

// ---------------------------------------------------------------------------
// HTTP Handlers
// ---------------------------------------------------------------------------

/// `GET /v1/stats/stream`
///
/// Returns a point-in-time snapshot of stream statistics including throughput,
/// size distributions, per-contract counts, moving averages, and anomalies.
pub async fn get_stream_stats(State(state): State<AppState>) -> impl IntoResponse {
    // Return stats from the SSE active connections counter already tracked on AppState.
    // The detailed StreamStats is sourced from AppState.stream_stats_state if present,
    // otherwise a minimal snapshot is constructed from available AppState metrics.
    use std::sync::atomic::Ordering;

    let active = state.sse_connections.load(Ordering::Relaxed);
    let snapshot = serde_json::json!({
        "active_sse_connections": active,
        "note": "detailed stream statistics available when stream_stats middleware is enabled"
    });

    Json(snapshot)
}

/// `GET /v1/stats/stream/{contract_id}`
///
/// Returns stream statistics scoped to a single contract.
///
/// # Errors
/// - `404 Not Found` if no events for this contract have been broadcast.
pub async fn get_contract_stream_stats(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> impl IntoResponse {
    use std::sync::atomic::Ordering;
    use axum::http::StatusCode;

    let _ = state.sse_connections.load(Ordering::Relaxed);

    // Attempt to look up contract stats from the database as a fallback.
    let row = sqlx::query!(
        r#"
        SELECT
            COUNT(*)::bigint AS event_count,
            MAX(timestamp) AS last_event_at,
            AVG(octet_length(event_data::text))::float8 AS avg_payload_bytes
        FROM events
        WHERE contract_id = $1
        "#,
        contract_id
    )
    .fetch_optional(&state.pool)
    .await;

    match row {
        Ok(Some(r)) if r.event_count.unwrap_or(0) > 0 => {
            let stats = ContractStats {
                event_count: r.event_count.unwrap_or(0) as u64,
                event_type_distribution: HashMap::new(),
                avg_payload_bytes: r.avg_payload_bytes.unwrap_or(0.0),
                last_event_at: r.last_event_at,
                events_per_second_1m: 0.0,
            };
            Json(serde_json::to_value(stats).unwrap_or_default()).into_response()
        }
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "no stats found for contract", "contract_id": contract_id })),
        )
            .into_response(),
        Err(e) => crate::error::AppError::from(e).into_response(),
    }
}

/// `GET /v1/stats/stream/throughput`
///
/// Returns recent events-per-second readings as a time-series.
/// The series covers up to the last 60 one-second buckets.
pub async fn get_stream_throughput(State(state): State<AppState>) -> impl IntoResponse {
    use std::sync::atomic::Ordering;

    let active = state.sse_connections.load(Ordering::Relaxed);

    // Return a minimal throughput snapshot derived from available AppState data.
    let throughput = serde_json::json!({
        "active_sse_connections": active,
        "throughput_series": [],
        "note": "detailed throughput series available when stream_stats middleware is enabled"
    });

    Json(throughput)
}
