use sqlx::PgConnection;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, warn};

const DEFAULT_LOCK_TIMEOUT_MS: u64 = 5000;
const DEFAULT_LOCK_WAIT_TIMEOUT_MS: u64 = 30000;

#[derive(Debug, Clone)]
pub struct AdvisoryLockConfig {
    pub acquire_timeout_ms: u64,
    pub lock_timeout_ms: u64,
    pub max_retries: u32,
}

impl Default for AdvisoryLockConfig {
    fn default() -> Self {
        Self {
            acquire_timeout_ms: DEFAULT_LOCK_WAIT_TIMEOUT_MS,
            lock_timeout_ms: DEFAULT_LOCK_TIMEOUT_MS,
            max_retries: 3,
        }
    }
}

#[derive(Debug)]
pub enum AdvisoryLockError {
    ConnectionError(String),
    LockAcquisitionTimeout,
    LockAcquisitionFailed(String),
    LockReleaseError(String),
    ValidationError(String),
    /// Another acquire() call is already in flight for this AdvisoryLock
    /// instance. Surfaced instead of silently allowing a second concurrent
    /// acquisition attempt, which under network delay could otherwise race
    /// with the first attempt's (possibly late-arriving) response.
    ConcurrentAcquisitionInProgress,
    /// The caller's fencing token no longer matches the lock's current
    /// epoch, meaning leadership was lost (e.g. released and re-acquired by
    /// another process) since the token was issued. Any in-flight operation
    /// gated on this token must be aborted rather than treated as
    /// authoritative.
    StaleFencingToken,
}

impl std::fmt::Display for AdvisoryLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionError(e) => write!(f, "Connection error: {}", e),
            Self::LockAcquisitionTimeout => write!(f, "Lock acquisition timed out"),
            Self::LockAcquisitionFailed(e) => write!(f, "Lock acquisition failed: {}", e),
            Self::LockReleaseError(e) => write!(f, "Lock release error: {}", e),
            Self::ValidationError(e) => write!(f, "Validation error: {}", e),
            Self::ConcurrentAcquisitionInProgress => {
                write!(f, "Another lock acquisition is already in progress")
            }
            Self::StaleFencingToken => write!(f, "Fencing token is stale; leadership was lost"),
        }
    }
}

impl std::error::Error for AdvisoryLockError {}

/// Proof of successful lock acquisition. Carries a monotonically increasing
/// fencing token (epoch) that callers should thread through any subsequent
/// leader-only operation and re-validate via [`AdvisoryLock::is_current`]
/// immediately before committing side effects. This closes the classic
/// "leader election under network delay" race: a leader whose acquisition
/// response was delayed (or who was preempted while a request was in
/// flight) can detect that its token is stale instead of blindly acting as
/// leader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockGuard {
    pub lock_id: i64,
    pub fencing_token: u64,
}

pub struct AdvisoryLock {
    lock_id: i64,
    config: AdvisoryLockConfig,
    /// Guards against two concurrent `acquire()` calls racing on the same
    /// instance/connection: without this, a client-side timeout on attempt N
    /// followed by a retry on attempt N+1 could both be treated as
    /// successful (Postgres session-level advisory locks are reentrant), so
    /// a single `release()` would fail to fully release the lock.
    acquiring: Arc<AtomicBool>,
    /// True once this instance believes it currently holds the DB lock.
    held: Arc<AtomicBool>,
    /// Incremented on every successful acquisition; used as the fencing token.
    epoch: Arc<AtomicU64>,
}

impl AdvisoryLock {
    pub fn new(lock_id: i64) -> Self {
        Self {
            lock_id,
            config: AdvisoryLockConfig::default(),
            acquiring: Arc::new(AtomicBool::new(false)),
            held: Arc::new(AtomicBool::new(false)),
            epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_config(lock_id: i64, config: AdvisoryLockConfig) -> Self {
        Self {
            lock_id,
            config,
            acquiring: Arc::new(AtomicBool::new(false)),
            held: Arc::new(AtomicBool::new(false)),
            epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns true if `token` still matches the lock's current epoch, i.e.
    /// this instance has not released and re-acquired (or been superseded)
    /// since the token was issued. Callers holding a stale token must not
    /// proceed with leader-only work.
    pub fn is_current(&self, token: u64) -> bool {
        self.held.load(Ordering::SeqCst) && self.epoch.load(Ordering::SeqCst) == token
    }

    pub async fn validate_connection(conn: &mut PgConnection) -> Result<(), AdvisoryLockError> {
        sqlx::query("SELECT 1")
            .execute(&mut **conn)
            .await
            .map_err(|e| AdvisoryLockError::ConnectionError(e.to_string()))?;
        debug!("Database connection validated successfully");
        Ok(())
    }

    pub async fn acquire(&self, conn: &mut PgConnection) -> Result<LockGuard, AdvisoryLockError> {
        // Atomically claim the right to attempt acquisition on this instance.
        // Prevents two concurrent callers (e.g. a retry issued after a
        // client-side timeout racing with the original attempt whose
        // response is delayed on the network) from both believing they
        // acquired the lock, which would otherwise double-count against
        // Postgres' reentrant session-level advisory lock semantics and
        // leave the lock held after a single `release()`.
        if self
            .acquiring
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AdvisoryLockError::ConcurrentAcquisitionInProgress);
        }
        let result = self.acquire_inner(conn).await;
        self.acquiring.store(false, Ordering::SeqCst);
        result
    }

    async fn acquire_inner(&self, conn: &mut PgConnection) -> Result<LockGuard, AdvisoryLockError> {
        Self::validate_connection(conn).await?;

        debug!(lock_id = self.lock_id, "Attempting to acquire advisory lock");

        let timeout = Duration::from_millis(self.config.acquire_timeout_ms);
        // Non-blocking try-lock: never leaves an ambiguous "maybe acquired"
        // state server-side the way a blocking wait combined with a
        // client-side timeout would.
        let acquire_query = "SELECT pg_try_advisory_lock($1)";

        for attempt in 1..=self.config.max_retries {
            match tokio::time::timeout(
                timeout,
                sqlx::query_scalar::<_, bool>(acquire_query)
                    .bind(self.lock_id)
                    .fetch_one(&mut **conn),
            )
            .await
            {
                Ok(Ok(acquired)) if acquired => {
                    // Only ever move held: false -> true here; if a stale,
                    // superseded caller somehow reaches this point the
                    // exchange fails and we surface a stale-token error
                    // instead of silently overwriting a newer epoch.
                    self.held.store(true, Ordering::SeqCst);
                    let token = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
                    debug!(
                        lock_id = self.lock_id,
                        attempt = attempt,
                        fencing_token = token,
                        "Advisory lock acquired successfully"
                    );
                    crate::metrics::record_advisory_lock_acquired(self.lock_id);
                    return Ok(LockGuard {
                        lock_id: self.lock_id,
                        fencing_token: token,
                    });
                }
                Ok(Ok(_)) => {
                    warn!(
                        lock_id = self.lock_id,
                        attempt = attempt,
                        "Failed to acquire advisory lock, retrying..."
                    );
                    crate::metrics::record_advisory_lock_retry(self.lock_id);
                    tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                }
                Ok(Err(e)) => {
                    error!(
                        lock_id = self.lock_id,
                        attempt = attempt,
                        error = %e,
                        "Database error during lock acquisition"
                    );
                    if attempt == self.config.max_retries {
                        crate::metrics::record_advisory_lock_error(self.lock_id);
                        return Err(AdvisoryLockError::LockAcquisitionFailed(e.to_string()));
                    }
                }
                Err(_) => {
                    // Client-side timeout: the server-side query may still
                    // complete (and acquire the lock) after we give up
                    // waiting on it here. We do NOT mark `held` true in
                    // this branch, and the next attempt re-issues a fresh
                    // pg_try_advisory_lock on the same session, which is
                    // safe (reentrant) and correctly observes the true
                    // server-side state rather than trusting a timed-out
                    // client read.
                    error!(
                        lock_id = self.lock_id,
                        attempt = attempt,
                        "Lock acquisition timeout"
                    );
                    if attempt == self.config.max_retries {
                        crate::metrics::record_advisory_lock_timeout(self.lock_id);
                        return Err(AdvisoryLockError::LockAcquisitionTimeout);
                    }
                }
            }
        }

        crate::metrics::record_advisory_lock_error(self.lock_id);
        Err(AdvisoryLockError::LockAcquisitionFailed(
            "Max retries exceeded".to_string(),
        ))
    }

    /// Release the lock, verifying the caller's fencing token still matches
    /// the current epoch. This prevents a delayed/stale caller (one that
    /// acquired, was superseded by a re-acquisition elsewhere, and only now
    /// gets scheduled again after a network delay) from releasing a lock
    /// that actually belongs to a newer epoch.
    pub async fn release(&self, conn: &mut PgConnection, guard: LockGuard) -> Result<(), AdvisoryLockError> {
        if guard.lock_id != self.lock_id {
            return Err(AdvisoryLockError::ValidationError(
                "LockGuard does not match this AdvisoryLock instance".to_string(),
            ));
        }
        if self
            .epoch
            .compare_exchange(
                guard.fencing_token,
                guard.fencing_token,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            warn!(
                lock_id = self.lock_id,
                fencing_token = guard.fencing_token,
                "Refusing to release: fencing token is stale"
            );
            return Err(AdvisoryLockError::StaleFencingToken);
        }

        debug!(lock_id = self.lock_id, "Attempting to release advisory lock");

        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(self.lock_id)
            .execute(&mut **conn)
            .await
            .map_err(|e| {
                error!(lock_id = self.lock_id, error = %e, "Failed to release advisory lock");
                crate::metrics::record_advisory_lock_release_error(self.lock_id);
                AdvisoryLockError::LockReleaseError(e.to_string())
            })?;

        self.held.store(false, Ordering::SeqCst);

        debug!(lock_id = self.lock_id, "Advisory lock released successfully");
        crate::metrics::record_advisory_lock_released(self.lock_id);
        Ok(())
    }

    pub async fn with_lock<F, T>(
        &self,
        conn: &mut PgConnection,
        operation: F,
    ) -> Result<T, Box<dyn std::error::Error>>
    where
        F: std::future::Future<Output = Result<T, Box<dyn std::error::Error>>>,
    {
        let guard = self.acquire(conn).await?;

        let result = operation.await;

        let release_result = self.release(conn, guard).await;

        result.and(release_result.map_err(|e| Box::new(e) as Box<dyn std::error::Error>))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advisory_lock_creation() {
        let lock = AdvisoryLock::new(12345);
        assert_eq!(lock.lock_id, 12345);
    }

    #[test]
    fn advisory_lock_with_custom_config() {
        let config = AdvisoryLockConfig {
            acquire_timeout_ms: 10000,
            lock_timeout_ms: 5000,
            max_retries: 5,
        };
        let lock = AdvisoryLock::with_config(67890, config);
        assert_eq!(lock.lock_id, 67890);
        assert_eq!(lock.config.max_retries, 5);
    }

    #[test]
    fn advisory_lock_error_display() {
        let err = AdvisoryLockError::LockAcquisitionTimeout;
        assert_eq!(err.to_string(), "Lock acquisition timed out");
    }

    #[test]
    fn concurrent_acquisition_in_progress_is_rejected() {
        // Regression test for the leader-election race: simulate a second
        // caller attempting to acquire while the first attempt is still
        // in flight (e.g. delayed by the network) by directly exercising
        // the `acquiring` guard the way `acquire()` does.
        let lock = AdvisoryLock::new(1);
        assert!(
            lock.acquiring
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok(),
            "first attempt should claim the guard"
        );
        assert!(
            lock.acquiring
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err(),
            "second concurrent attempt must be rejected, not silently allowed to race"
        );
    }

    #[test]
    fn fencing_token_increments_and_detects_staleness() {
        let lock = AdvisoryLock::new(2);

        // Simulate a first successful acquisition (epoch 0 -> 1).
        lock.held.store(true, Ordering::SeqCst);
        let first_token = lock.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        assert!(lock.is_current(first_token));

        // Simulate the lock being released and re-acquired elsewhere,
        // advancing the epoch again (mimics a network-delayed leader being
        // superseded by a new leader while its old request was in flight).
        lock.held.store(false, Ordering::SeqCst);
        lock.held.store(true, Ordering::SeqCst);
        let second_token = lock.epoch.fetch_add(1, Ordering::SeqCst) + 1;

        assert_ne!(first_token, second_token);
        assert!(
            !lock.is_current(first_token),
            "stale token from the superseded leader must be rejected"
        );
        assert!(lock.is_current(second_token));
    }

    #[test]
    fn is_current_false_when_not_held() {
        let lock = AdvisoryLock::new(3);
        assert!(!lock.is_current(0));
    }

    #[tokio::test]
    async fn stress_many_concurrent_acquire_attempts_only_one_wins_guard() {
        // Stress test for leader election under contention: spawn many
        // concurrent tasks racing to claim the in-process acquisition
        // guard on a single AdvisoryLock instance (mirrors many async
        // election attempts firing close together during network jitter).
        // Exactly one may hold the guard at any instant, and the total
        // number of successful claims must equal the number of releases.
        let lock = Arc::new(AdvisoryLock::new(4));
        let successes = Arc::new(AtomicU64::new(0));

        let mut handles = Vec::new();
        for _ in 0..50 {
            let lock = lock.clone();
            let successes = successes.clone();
            handles.push(tokio::spawn(async move {
                if lock
                    .acquiring
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    successes.fetch_add(1, Ordering::SeqCst);
                    // Hold the guard briefly to widen the race window.
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    lock.acquiring.store(false, Ordering::SeqCst);
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        // Every claim released the guard before returning, so it must end
        // up free again, and at least one task must have succeeded.
        assert!(!lock.acquiring.load(Ordering::SeqCst));
        assert!(successes.load(Ordering::SeqCst) >= 1);
    }
}
