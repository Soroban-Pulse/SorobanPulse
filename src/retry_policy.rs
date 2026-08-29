use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

/// Retry strategy enum for different backoff patterns.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RetryStrategy {
    #[serde(rename = "exponential")]
    Exponential,
    #[serde(rename = "linear")]
    Linear,
    #[serde(rename = "fixed")]
    Fixed,
}

impl RetryStrategy {
    pub fn calculate_backoff(&self, attempt: u32, initial_ms: u64, multiplier: f64) -> u64 {
        match self {
            RetryStrategy::Exponential => {
                (initial_ms as f64 * multiplier.powi((attempt - 1) as i32)) as u64
            }
            RetryStrategy::Linear => initial_ms * attempt as u64,
            RetryStrategy::Fixed => initial_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff_ms: u64,
    pub backoff_multiplier: f64,
    pub max_backoff_ms: u64,
    #[serde(default)]
    pub strategy: Option<RetryStrategy>,
    #[serde(default)]
    pub use_jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 1000,
            backoff_multiplier: 2.0,
            max_backoff_ms: 60000,
            strategy: Some(RetryStrategy::Exponential),
            use_jitter: false,
        }
    }
}

impl RetryPolicy {
    pub fn webhook_default() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff_ms: 1000,
            backoff_multiplier: 2.0,
            max_backoff_ms: 600_000,
            strategy: Some(RetryStrategy::Exponential),
            use_jitter: true,
        }
    }

    pub fn email_default() -> Self {
        Self {
            max_attempts: 1,
            initial_backoff_ms: 0,
            backoff_multiplier: 1.0,
            max_backoff_ms: 0,
            strategy: Some(RetryStrategy::Fixed),
            use_jitter: false,
        }
    }

    pub fn sms_default() -> Self {
        Self {
            max_attempts: 2,
            initial_backoff_ms: 2000,
            backoff_multiplier: 1.5,
            max_backoff_ms: 10000,
            strategy: Some(RetryStrategy::Linear),
            use_jitter: true,
        }
    }

    /// Calculate backoff duration with optional jitter to prevent thundering herd.
    pub fn calculate_backoff(&self, attempt: u32) -> Duration {
        if attempt == 0 || self.initial_backoff_ms == 0 {
            return Duration::from_millis(0);
        }

        let strategy = self.strategy.unwrap_or(RetryStrategy::Exponential);
        let backoff_ms = strategy.calculate_backoff(attempt, self.initial_backoff_ms, self.backoff_multiplier);
        let mut capped_backoff = backoff_ms.min(self.max_backoff_ms);

        if self.use_jitter {
            capped_backoff = Self::apply_jitter(capped_backoff);
        }

        Duration::from_millis(capped_backoff)
    }

    /// Apply full jitter: random value between 0 and backoff_ms.
    /// This prevents thundering herd problem where multiple clients retry simultaneously.
    fn apply_jitter(backoff_ms: u64) -> u64 {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};

        let mut hasher = RandomState::new().build_hasher();
        hasher.write_usize(std::process::id() as usize);
        hasher.write_u64(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64);

        let hash = hasher.finish();
        let jitter_ratio = (hash as f64 / u64::MAX as f64).clamp(0.0, 1.0);
        (backoff_ms as f64 * jitter_ratio) as u64
    }

    pub async fn execute_with_retry<F, Fut, T, E>(&self, mut operation: F) -> Result<T, E>
    where
        F: FnMut(u32) -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut last_error = None;

        for attempt in 1..=self.max_attempts {
            match operation(attempt).await {
                Ok(result) => return Ok(result),
                Err(error) => {
                    if attempt < self.max_attempts {
                        let backoff = self.calculate_backoff(attempt);
                        warn!(
                            attempt = attempt,
                            max_attempts = self.max_attempts,
                            backoff_ms = backoff.as_millis(),
                            error = %error,
                            "Operation failed, retrying after backoff"
                        );
                        sleep(backoff).await;
                    }
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_backoff_exponential() {
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff_ms: 1000,
            backoff_multiplier: 2.0,
            max_backoff_ms: 5000,
            strategy: Some(RetryStrategy::Exponential),
            use_jitter: false,
        };

        assert_eq!(policy.calculate_backoff(0), Duration::from_millis(0));
        assert_eq!(policy.calculate_backoff(1), Duration::from_millis(1000));
        assert_eq!(policy.calculate_backoff(2), Duration::from_millis(2000));
        assert_eq!(policy.calculate_backoff(3), Duration::from_millis(4000));

        let policy_with_cap = RetryPolicy {
            max_attempts: 5,
            initial_backoff_ms: 1000,
            backoff_multiplier: 2.0,
            max_backoff_ms: 3000,
            strategy: Some(RetryStrategy::Exponential),
            use_jitter: false,
        };
        assert_eq!(policy_with_cap.calculate_backoff(3), Duration::from_millis(3000));
        assert_eq!(policy_with_cap.calculate_backoff(4), Duration::from_millis(3000));
    }

    #[test]
    fn test_calculate_backoff_linear() {
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff_ms: 1000,
            backoff_multiplier: 1.0,
            max_backoff_ms: 10000,
            strategy: Some(RetryStrategy::Linear),
            use_jitter: false,
        };

        assert_eq!(policy.calculate_backoff(1), Duration::from_millis(1000));
        assert_eq!(policy.calculate_backoff(2), Duration::from_millis(2000));
        assert_eq!(policy.calculate_backoff(3), Duration::from_millis(3000));
    }

    #[test]
    fn test_calculate_backoff_fixed() {
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff_ms: 1000,
            backoff_multiplier: 1.0,
            max_backoff_ms: 1000,
            strategy: Some(RetryStrategy::Fixed),
            use_jitter: false,
        };

        assert_eq!(policy.calculate_backoff(1), Duration::from_millis(1000));
        assert_eq!(policy.calculate_backoff(2), Duration::from_millis(1000));
        assert_eq!(policy.calculate_backoff(3), Duration::from_millis(1000));
    }

    #[test]
    fn test_calculate_backoff_with_jitter() {
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff_ms: 1000,
            backoff_multiplier: 2.0,
            max_backoff_ms: 5000,
            strategy: Some(RetryStrategy::Exponential),
            use_jitter: true,
        };

        let d1 = policy.calculate_backoff(1);
        let d2 = policy.calculate_backoff(2);

        assert!(d1 <= Duration::from_millis(1000), "Jittered backoff should not exceed base");
        assert!(d2 <= Duration::from_millis(2000), "Jittered backoff should not exceed base");
    }

    #[tokio::test]
    async fn test_execute_with_retry_success() {
        let policy = RetryPolicy::default();
        let mut call_count = 0;

        let result = policy.execute_with_retry(|_attempt| {
            call_count += 1;
            async move {
                if call_count < 2 {
                    Err("temporary error")
                } else {
                    Ok("success")
                }
            }
        }).await;

        assert_eq!(result, Ok("success"));
        assert_eq!(call_count, 2);
    }

    #[tokio::test]
    async fn test_execute_with_retry_failure() {
        let policy = RetryPolicy {
            max_attempts: 2,
            initial_backoff_ms: 1,
            backoff_multiplier: 1.0,
            max_backoff_ms: 1,
            strategy: Some(RetryStrategy::Exponential),
            use_jitter: false,
        };
        let mut call_count = 0;

        let result = policy.execute_with_retry(|_attempt| {
            call_count += 1;
            async move { Err("persistent error") }
        }).await;

        assert_eq!(result, Err("persistent error"));
        assert_eq!(call_count, 2);
    }

    #[test]
    fn test_retry_strategy_calculation() {
        assert_eq!(RetryStrategy::Exponential.calculate_backoff(1, 1000, 2.0), 1000);
        assert_eq!(RetryStrategy::Exponential.calculate_backoff(2, 1000, 2.0), 2000);
        assert_eq!(RetryStrategy::Linear.calculate_backoff(2, 1000, 1.0), 2000);
        assert_eq!(RetryStrategy::Fixed.calculate_backoff(5, 1000, 1.0), 1000);
    }

    #[test]
    fn test_jitter_is_less_than_base() {
        let backoff = 1000u64;
        let jittered = RetryPolicy::apply_jitter(backoff);
        assert!(jittered <= backoff, "Jittered value should not exceed base");
    }
}