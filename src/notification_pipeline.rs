//! Issue #1467 — Unified notification pipeline consolidating:
//! - notification_formatter: Format by channel type
//! - notification_dedup: Deduplicate messages
//! - notification_rate_limit: Apply global rate limits
//! - notification_channel: Route to channel handler
//! - Channel handler: Channel-specific logic with unified guards
//! - notification_delivery: Track delivery status
//! - notification_admin: Admin logging/alerting
//!
//! Provides common patterns for all channels:
//! - SSRF guard: Outbound URL validation (applies to all, not just Discord)
//! - Retry policy: Unified exponential backoff
//! - Rate limiting: Per-channel and global limits

use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Channel type for routing and configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationChannel {
    Email,
    Sms,
    Push,
    Slack,
    Teams,
    Telegram,
    Discord,
    PagerDuty,
    Webhook,
    EventBridge,
    Sqs,
}

/// Retry policy with unified exponential backoff
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    max_attempts: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
    backoff_multiplier: f64,
}

impl RetryPolicy {
    /// Create a default retry policy for a channel
    pub fn for_channel(channel: NotificationChannel) -> Self {
        match channel {
            NotificationChannel::Email => Self {
                max_attempts: 5,
                initial_backoff: Duration::from_secs(1),
                max_backoff: Duration::from_secs(300),
                backoff_multiplier: 2.0,
            },
            NotificationChannel::Sms => Self {
                max_attempts: 3,
                initial_backoff: Duration::from_secs(2),
                max_backoff: Duration::from_secs(60),
                backoff_multiplier: 2.0,
            },
            NotificationChannel::Discord | NotificationChannel::Slack => Self {
                max_attempts: 4,
                initial_backoff: Duration::from_secs(1),
                max_backoff: Duration::from_secs(120),
                backoff_multiplier: 2.0,
            },
            NotificationChannel::Webhook => Self {
                max_attempts: 5,
                initial_backoff: Duration::from_secs(1),
                max_backoff: Duration::from_secs(300),
                backoff_multiplier: 2.0,
            },
            _ => Self {
                max_attempts: 3,
                initial_backoff: Duration::from_secs(1),
                max_backoff: Duration::from_secs(60),
                backoff_multiplier: 2.0,
            },
        }
    }

    /// Calculate backoff duration for attempt N
    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }
        let backoff_secs =
            self.initial_backoff.as_secs_f64() * self.backoff_multiplier.powi((attempt - 1) as i32);
        let capped = backoff_secs.min(self.max_backoff.as_secs_f64());
        Duration::from_secs_f64(capped)
    }
}

/// SSRF (Server-Side Request Forgery) guard for outbound URLs
/// Issue #1467: Apply to all channels, not just Discord
pub struct SsrfGuard {
    allowed_domains: Vec<String>,
    blocked_ips: Vec<String>,
}

impl SsrfGuard {
    /// Create default SSRF guard
    pub fn new() -> Self {
        Self {
            // Whitelist public webhook domains
            allowed_domains: vec![
                "discord.com".to_string(),
                "discordapp.com".to_string(),
                "slack.com".to_string(),
                "api.telegram.org".to_string(),
                "graph.microsoft.com".to_string(),
                "api.pagerduty.com".to_string(),
            ],
            // Block private IP ranges
            blocked_ips: vec![
                "127.0.0.1".to_string(),
                "0.0.0.0".to_string(),
                "169.254".to_string(),
                "172.16".to_string(),
                "192.168".to_string(),
                "10.0".to_string(),
            ],
        }
    }

    /// Validate URL is safe for outbound requests
    pub fn validate_url(&self, url: &str) -> Result<(), String> {
        // Parse and check domain
        let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

        // Extract domain
        let domain = parsed
            .host_str()
            .ok_or_else(|| "Missing host in URL".to_string())?;

        // Check IP address is not private
        if let Ok(ip) = domain.parse::<std::net::IpAddr>() {
            if ip.is_loopback() || ip.is_private() || ip.is_link_local() {
                return Err(format!("Private IP address not allowed: {}", ip));
            }
        }

        // Check domain is whitelisted or allow public domains
        let is_allowed = self.allowed_domains.iter().any(|d| domain.ends_with(d))
            || !domain.starts_with("internal")
            && !domain.starts_with("localhost")
            && !domain.contains(".local");

        if !is_allowed {
            return Err(format!("Domain not whitelisted: {}", domain));
        }

        Ok(())
    }
}

impl Default for SsrfGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Notification message in the pipeline
#[derive(Debug, Clone)]
pub struct NotificationMessage {
    pub id: Uuid,
    pub channel: NotificationChannel,
    pub recipient: String,
    pub subject: Option<String>,
    pub body: String,
    pub metadata: std::collections::HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Pipeline context for notification processing
pub struct NotificationPipelineContext {
    pub ssrf_guard: Arc<SsrfGuard>,
    pub rate_limiter: Arc<RateLimiter>,
}

/// Unified rate limiter for all channels
#[derive(Debug)]
pub struct RateLimiter {
    limits: std::collections::HashMap<NotificationChannel, (u32, Duration)>,
}

impl RateLimiter {
    /// Create default rate limiter with channel-specific limits
    pub fn new() -> Self {
        let mut limits = std::collections::HashMap::new();

        // Email: 100/min per recipient
        limits.insert(NotificationChannel::Email, (100, Duration::from_secs(60)));

        // SMS: 10/min per recipient
        limits.insert(NotificationChannel::Sms, (10, Duration::from_secs(60)));

        // Discord/Slack: 30/min per channel
        limits.insert(NotificationChannel::Discord, (30, Duration::from_secs(60)));
        limits.insert(NotificationChannel::Slack, (30, Duration::from_secs(60)));

        // Webhook: 100/min per endpoint
        limits.insert(NotificationChannel::Webhook, (100, Duration::from_secs(60)));

        Self { limits }
    }

    /// Get rate limit for channel
    pub fn get_limit(&self, channel: NotificationChannel) -> (u32, Duration) {
        self.limits.get(&channel).copied().unwrap_or((100, Duration::from_secs(60)))
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Notification delivery status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStatus {
    Pending,
    Sent,
    Failed,
    Bounced,
    Deferred,
}

/// Delivery result
#[derive(Debug, Clone)]
pub struct DeliveryResult {
    pub message_id: Uuid,
    pub channel: NotificationChannel,
    pub status: DeliveryStatus,
    pub attempt: u32,
    pub error_message: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Pipeline trait for channel handlers
pub trait NotificationChannelHandler: Send + Sync {
    /// Send notification via this channel
    async fn send(
        &self,
        message: &NotificationMessage,
        context: &NotificationPipelineContext,
    ) -> Result<DeliveryResult, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_backoff() {
        let policy = RetryPolicy::for_channel(NotificationChannel::Email);
        assert_eq!(policy.backoff_for_attempt(0), Duration::ZERO);
        assert!(policy.backoff_for_attempt(1) >= Duration::from_secs(1));
        assert!(policy.backoff_for_attempt(2) >= Duration::from_secs(2));
    }

    #[test]
    fn test_ssrf_guard_blocks_private_ips() {
        let guard = SsrfGuard::new();
        assert!(guard.validate_url("http://127.0.0.1:8080").is_err());
        assert!(guard.validate_url("http://192.168.1.1").is_err());
        assert!(guard.validate_url("http://10.0.0.1").is_err());
    }

    #[test]
    fn test_ssrf_guard_allows_public_urls() {
        let guard = SsrfGuard::new();
        assert!(guard.validate_url("https://discord.com/api/webhooks/123").is_ok());
        assert!(guard.validate_url("https://slack.com/api/chat.postMessage").is_ok());
    }

    #[test]
    fn test_rate_limiter_defaults() {
        let limiter = RateLimiter::new();
        let (limit, window) = limiter.get_limit(NotificationChannel::Email);
        assert_eq!(limit, 100);
        assert_eq!(window, Duration::from_secs(60));
    }
}
