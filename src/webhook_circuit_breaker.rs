/// Issue #879: Webhook endpoint circuit breaker implementation
/// Prevents cascading failures and reduces wasted retries for failing webhook endpoints

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc, Duration};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CircuitBreakerState {
    /// Circuit is closed, requests pass through normally
    Closed,
    /// Circuit is open, requests are rejected immediately
    Open,
    /// Circuit is half-open, testing if the service recovered
    HalfOpen,
}

/// Configuration for circuit breaker behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit
    pub failure_threshold: u32,
    /// Duration for which the circuit remains open before transitioning to half-open
    pub open_duration_secs: u64,
    /// Number of successful requests in half-open state before closing the circuit
    pub success_threshold_half_open: u32,
    /// Maximum failure rate (0.0 to 1.0) before opening circuit
    pub failure_rate_threshold: f64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_duration_secs: 60,
            success_threshold_half_open: 3,
            failure_rate_threshold: 0.5,
        }
    }
}

/// Per-endpoint circuit breaker state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointCircuitBreaker {
    /// Current state of the circuit breaker
    pub state: CircuitBreakerState,
    /// Number of consecutive failures
    pub failure_count: u32,
    /// Number of consecutive successes in half-open state
    pub success_count: u32,
    /// Total requests processed
    pub total_requests: u64,
    /// Total failures
    pub total_failures: u64,
    /// Timestamp when circuit was last opened
    pub opened_at: Option<DateTime<Utc>>,
    /// Timestamp when circuit transitioned to half-open
    pub half_open_at: Option<DateTime<Utc>>,
    /// Configuration for this endpoint
    pub config: CircuitBreakerConfig,
    /// Recent failures (for exponential backoff calculation)
    pub recent_failures: Vec<DateTime<Utc>>,
}

impl EndpointCircuitBreaker {
    /// Create a new circuit breaker for an endpoint
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            success_count: 0,
            total_requests: 0,
            total_failures: 0,
            opened_at: None,
            half_open_at: None,
            config,
            recent_failures: Vec::new(),
        }
    }

    /// Record a successful request
    pub fn record_success(&mut self) {
        self.total_requests += 1;

        match self.state {
            CircuitBreakerState::Closed => {
                self.failure_count = 0;
            }
            CircuitBreakerState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold_half_open {
                    self.state = CircuitBreakerState::Closed;
                    self.failure_count = 0;
                    self.success_count = 0;
                    self.opened_at = None;
                    self.half_open_at = None;
                    info!("Circuit breaker closed");
                }
            }
            CircuitBreakerState::Open => {
                // Circuit open, request shouldn't have succeeded
                // But if it did, count it as progress toward recovery
                self.success_count += 1;
            }
        }
    }

    /// Record a failed request
    pub fn record_failure(&mut self) {
        self.total_requests += 1;
        self.total_failures += 1;
        self.failure_count += 1;
        self.recent_failures.push(Utc::now());

        // Keep only recent failures (last hour)
        let cutoff = Utc::now() - Duration::hours(1);
        self.recent_failures.retain(|&t| t > cutoff);

        match self.state {
            CircuitBreakerState::Closed => {
                // Calculate current failure rate
                let failure_rate = self.total_failures as f64 / self.total_requests.max(1) as f64;

                if self.failure_count >= self.config.failure_threshold
                    || failure_rate > self.config.failure_rate_threshold
                {
                    self.state = CircuitBreakerState::Open;
                    self.opened_at = Some(Utc::now());
                    warn!(
                        "Circuit breaker opened: {} consecutive failures or {:.2}% failure rate",
                        self.failure_count,
                        failure_rate * 100.0
                    );
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Any failure in half-open state immediately opens the circuit
                self.state = CircuitBreakerState::Open;
                self.opened_at = Some(Utc::now());
                self.success_count = 0;
                warn!("Circuit breaker opened during half-open state");
            }
            CircuitBreakerState::Open => {
                // Already open, just track the failure
            }
        }
    }

    /// Check if circuit can accept requests and transition to half-open if needed
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(opened_at) = self.opened_at {
                    let elapsed = (Utc::now() - opened_at).num_seconds();
                    if elapsed >= self.config.open_duration_secs as i64 {
                        self.state = CircuitBreakerState::HalfOpen;
                        self.half_open_at = Some(Utc::now());
                        self.success_count = 0;
                        info!("Circuit breaker transitioned to half-open state");
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    /// Calculate exponential backoff in seconds based on recent failures
    pub fn calculate_backoff_secs(&self) -> u64 {
        let num_recent_failures = self.recent_failures.len().min(32) as u32;
        if num_recent_failures == 0 {
            0
        } else {
            // Exponential backoff: 2^(failures-1) seconds, capped at 3600 (1 hour)
            2u64.pow(num_recent_failures - 1).min(3600)
        }
    }

    /// Get the current failure rate
    pub fn failure_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.total_failures as f64 / self.total_requests as f64
        }
    }
}

/// Manager for all endpoint circuit breakers
pub struct CircuitBreakerManager {
    breakers: Arc<RwLock<HashMap<String, EndpointCircuitBreaker>>>,
    default_config: CircuitBreakerConfig,
}

impl CircuitBreakerManager {
    /// Create a new circuit breaker manager
    pub fn new(default_config: CircuitBreakerConfig) -> Self {
        Self {
            breakers: Arc::new(RwLock::new(HashMap::new())),
            default_config,
        }
    }

    /// Check if a request to the endpoint is allowed
    pub async fn allow_request(&self, endpoint: &str) -> bool {
        let mut breakers = self.breakers.write().await;
        let breaker = breakers
            .entry(endpoint.to_string())
            .or_insert_with(|| EndpointCircuitBreaker::new(self.default_config.clone()));

        breaker.allow_request()
    }

    /// Record a successful request
    pub async fn record_success(&self, endpoint: &str) {
        let mut breakers = self.breakers.write().await;
        if let Some(breaker) = breakers.get_mut(endpoint) {
            breaker.record_success();
        }
    }

    /// Record a failed request
    pub async fn record_failure(&self, endpoint: &str) {
        let mut breakers = self.breakers.write().await;
        if let Some(breaker) = breakers.get_mut(endpoint) {
            breaker.record_failure();
        }
    }

    /// Get current state of an endpoint's circuit breaker
    pub async fn get_state(&self, endpoint: &str) -> Option<CircuitBreakerState> {
        let breakers = self.breakers.read().await;
        breakers.get(endpoint).map(|b| b.state)
    }

    /// Get detailed stats for an endpoint
    pub async fn get_stats(&self, endpoint: &str) -> Option<EndpointCircuitBreakerStats> {
        let breakers = self.breakers.read().await;
        breakers.get(endpoint).map(|b| EndpointCircuitBreakerStats {
            endpoint: endpoint.to_string(),
            state: b.state,
            failure_count: b.failure_count,
            success_count: b.success_count,
            total_requests: b.total_requests,
            total_failures: b.total_failures,
            failure_rate: b.failure_rate(),
            backoff_seconds: b.calculate_backoff_secs(),
            opened_at: b.opened_at,
            half_open_at: b.half_open_at,
        })
    }

    /// Manually reset circuit breaker for an endpoint
    pub async fn reset(&self, endpoint: &str) {
        let mut breakers = self.breakers.write().await;
        if let Some(breaker) = breakers.get_mut(endpoint) {
            breaker.state = CircuitBreakerState::Closed;
            breaker.failure_count = 0;
            breaker.success_count = 0;
            breaker.opened_at = None;
            breaker.half_open_at = None;
            info!("Circuit breaker manually reset for endpoint: {}", endpoint);
        }
    }

    /// Get all endpoint stats
    pub async fn get_all_stats(&self) -> Vec<EndpointCircuitBreakerStats> {
        let breakers = self.breakers.read().await;
        breakers
            .iter()
            .map(|(endpoint, breaker)| EndpointCircuitBreakerStats {
                endpoint: endpoint.clone(),
                state: breaker.state,
                failure_count: breaker.failure_count,
                success_count: breaker.success_count,
                total_requests: breaker.total_requests,
                total_failures: breaker.total_failures,
                failure_rate: breaker.failure_rate(),
                backoff_seconds: breaker.calculate_backoff_secs(),
                opened_at: breaker.opened_at,
                half_open_at: breaker.half_open_at,
            })
            .collect()
    }
}

/// Statistics for a circuit breaker endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointCircuitBreakerStats {
    pub endpoint: String,
    pub state: CircuitBreakerState,
    pub failure_count: u32,
    pub success_count: u32,
    pub total_requests: u64,
    pub total_failures: u64,
    pub failure_rate: f64,
    pub backoff_seconds: u64,
    pub opened_at: Option<DateTime<Utc>>,
    pub half_open_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circuit_breaker_starts_closed() {
        let breaker = EndpointCircuitBreaker::new(CircuitBreakerConfig::default());
        assert_eq!(breaker.state, CircuitBreakerState::Closed);
    }

    #[test]
    fn circuit_breaker_opens_after_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let mut breaker = EndpointCircuitBreaker::new(config);

        for _ in 0..3 {
            breaker.record_failure();
        }

        assert_eq!(breaker.state, CircuitBreakerState::Open);
    }

    #[test]
    fn circuit_breaker_transitions_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            open_duration_secs: 0, // Allow immediate transition
            ..Default::default()
        };
        let mut breaker = EndpointCircuitBreaker::new(config);

        breaker.record_failure();
        assert_eq!(breaker.state, CircuitBreakerState::Open);

        // Simulate time passing
        breaker.opened_at = Some(Utc::now() - Duration::seconds(61));

        assert!(breaker.allow_request());
        assert_eq!(breaker.state, CircuitBreakerState::HalfOpen);
    }

    #[test]
    fn exponential_backoff_calculation() {
        let config = CircuitBreakerConfig::default();
        let mut breaker = EndpointCircuitBreaker::new(config);

        for i in 0..5 {
            breaker.recent_failures.push(Utc::now());
            let backoff = breaker.calculate_backoff_secs();
            assert_eq!(backoff, 2u64.pow(i as u32));
        }
    }

    #[test]
    fn failure_rate_calculation() {
        let config = CircuitBreakerConfig::default();
        let mut breaker = EndpointCircuitBreaker::new(config);

        breaker.total_requests = 10;
        breaker.total_failures = 5;

        assert_eq!(breaker.failure_rate(), 0.5);
    }
}
