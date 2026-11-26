# Issue #1467 — Notification Pipeline Consolidation

## Overview

The notification subsystem is now unified through `src/notification_pipeline.rs`, which provides:

- **RetryPolicy**: Unified exponential backoff for all channels
- **SsrfGuard**: Outbound URL validation (applies to all, not just Discord)
- **RateLimiter**: Per-channel rate limiting
- **NotificationChannelHandler**: Trait for channel implementations
- **DeliveryResult**: Standardized delivery status tracking

## Architecture

```
Input Message
    ↓
[Formatter] ← notification_formatter.rs
    ↓ Formatted by channel type
[Dedup] ← notification_dedup.rs
    ↓ Remove duplicates
[RateLimit] ← notification_rate_limit.rs (now uses RateLimiter from pipeline)
    ↓ Check global/per-channel limits
[Channel Router] ← notification_channel.rs
    ↓ Route to handler (Email, SMS, Discord, etc.)
[Handler] ← Implements NotificationChannelHandler
    ├─ [SsrfGuard] ← Validate outbound URLs
    ├─ [RetryPolicy] ← Apply unified retry strategy
    ├─ [RateCheck] ← Verify rate limits
    └─ [Send] ← Send via channel
    ↓
[Delivery] ← notification_delivery.rs
    ↓ Track status
[Admin] ← notification_admin.rs
    ↓ Log/alert
Output (sent, bounced, deferred, failed)
```

## Migration Path

### Phase 1: Foundation (DONE ✅)
- [x] Create `notification_pipeline.rs` with unified patterns
- [x] Add to lib.rs exports
- [x] Include tests for RetryPolicy, SsrfGuard, RateLimiter

### Phase 2: Channel Adoption (NEXT)
Implement for each channel in order:

1. **Discord** (simplest, already has SSRF guard)
   - Implement `NotificationChannelHandler` trait
   - Use `RetryPolicy::for_channel(Discord)` 
   - Use `SsrfGuard` from pipeline
   
2. **Slack** (similar to Discord)
   - Implement `NotificationChannelHandler` trait
   - Use `RetryPolicy::for_channel(Slack)`
   - Use `SsrfGuard` from pipeline

3. **Email**
   - Implement `NotificationChannelHandler` trait
   - Use `RetryPolicy::for_channel(Email)` (5 attempts, 5min max backoff)
   - Use `SsrfGuard` for external SMTP

4. **SMS**
   - Implement `NotificationChannelHandler` trait
   - Use `RetryPolicy::for_channel(Sms)` (3 attempts, 1min max)
   - Use `SsrfGuard` for gateway URLs

5. **Webhook**
   - Implement `NotificationChannelHandler` trait
   - Use `RetryPolicy::for_channel(Webhook)`
   - Use `SsrfGuard` (required for user-supplied URLs)

6. **Other channels** (Teams, Telegram, PagerDuty, EventBridge, SQS, Push)
   - Follow same pattern

### Phase 3: Consolidation (AFTER all channels migrated)
- Remove individual retry logic from each channel
- Remove per-channel SSRF checks (use pipeline)
- Deprecate old `notification_rate_limit.rs` patterns
- Update `notification_channel.rs` to use pipeline routes

## Implementation Example

```rust
// Before: Per-channel implementation
impl SlackHandler {
    pub async fn send(&self, msg: &Message) -> Result<()> {
        for attempt in 0..4 {
            match self.send_internal(msg).await {
                Ok(_) => return Ok(()),
                Err(e) if attempt < 3 => {
                    let backoff = Duration::from_secs(2_u64.pow(attempt));
                    tokio::time::sleep(backoff).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

// After: Using pipeline
impl NotificationChannelHandler for SlackHandler {
    async fn send(
        &self,
        msg: &NotificationMessage,
        ctx: &NotificationPipelineContext,
    ) -> Result<DeliveryResult> {
        let policy = RetryPolicy::for_channel(NotificationChannel::Slack);
        
        for attempt in 0..policy.max_attempts {
            if attempt > 0 {
                let backoff = policy.backoff_for_attempt(attempt);
                tokio::time::sleep(backoff).await;
            }
            
            // Validate webhook URL with SSRF guard
            ctx.ssrf_guard.validate_url(&self.webhook_url)?;
            
            match self.send_internal(msg).await {
                Ok(_) => return Ok(DeliveryResult {
                    message_id: msg.id,
                    channel: NotificationChannel::Slack,
                    status: DeliveryStatus::Sent,
                    attempt,
                    error_message: None,
                    timestamp: chrono::Utc::now(),
                }),
                Err(e) if attempt < policy.max_attempts as u32 - 1 => continue,
                Err(e) => return Ok(DeliveryResult {
                    message_id: msg.id,
                    channel: NotificationChannel::Slack,
                    status: DeliveryStatus::Failed,
                    attempt,
                    error_message: Some(e.to_string()),
                    timestamp: chrono::Utc::now(),
                }),
            }
        }
    }
}
```

## Benefits

✅ **Consistency**: All channels use same retry/rate-limit/SSRF logic  
✅ **Security**: SSRF guard applies everywhere, not just Discord  
✅ **Maintainability**: Single place to update retry policies  
✅ **Testability**: Pipeline logic tested independently  
✅ **Performance**: Unified rate limiter with shared state  

## Testing

```bash
cargo test notification_pipeline
```

Tests include:
- Retry policy backoff calculation
- SSRF guard validation for public/private URLs
- Rate limiter defaults and channel-specific limits
