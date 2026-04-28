# Email Notification Feature - Implementation Summary

## Overview

This document summarizes the implementation of the email notification feature for Soroban Pulse, which allows operators to receive email alerts when specific events are indexed.

## Changes Made

### 1. Dependencies (`Cargo.toml`)

Added the `lettre` crate for SMTP email sending:
```toml
lettre = { version = "0.11", default-features = false, features = ["tokio1-rustls-tls", "smtp-transport", "builder"] }
```

### 2. Configuration (`src/config.rs`)

Added email configuration fields to the `Config` struct:
- `email_smtp_host: Option<String>` - SMTP server hostname
- `email_smtp_port: u16` - SMTP server port (default: 587)
- `email_smtp_user: Option<String>` - SMTP authentication username
- `email_smtp_password: Option<String>` - SMTP authentication password (never logged)
- `email_from: Option<String>` - Sender email address
- `email_to: Vec<String>` - List of recipient email addresses
- `email_contract_filter: Vec<String>` - Optional contract ID filter

Configuration is loaded from environment variables with proper parsing and validation.

### 3. Email Module (`src/email.rs`)

Created a new `EmailNotifier` struct that:
- Subscribes to the event broadcast channel
- Batches events for up to 60 seconds
- Sends a single summary email per minute
- Applies contract filtering when configured
- Groups events by contract ID in the email body
- Handles SMTP authentication and TLS
- Records failures to Prometheus metrics

Key features:
- Asynchronous email sending using `tokio::task::spawn_blocking`
- Graceful handling of channel lag and closure
- Detailed event summaries with up to 10 events per contract shown
- Secure credential handling (passwords never logged)

### 4. Metrics (`src/metrics.rs`)

Added a new Prometheus counter:
```rust
pub fn record_email_failure() {
    m::counter!("soroban_pulse_email_failures_total", 1u64);
}
```

This metric tracks email delivery failures for monitoring and alerting.

### 5. Main Application (`src/main.rs`)

Integrated email notifications into the main application:
- Added `email` module import
- Spawns email notifier task when `EMAIL_SMTP_HOST` is configured
- Validates that `EMAIL_FROM` and `EMAIL_TO` are set
- Logs configuration at startup
- Subscribes to the event broadcast channel

### 6. Module Exports (`src/lib.rs`)

Added `pub mod email;` to expose the email module for testing.

### 7. Environment Configuration (`.env.example`)

Documented all email configuration variables:
- `EMAIL_SMTP_HOST` - SMTP server hostname
- `EMAIL_SMTP_PORT` - SMTP server port (default: 587)
- `EMAIL_SMTP_USER` - SMTP username
- `EMAIL_SMTP_PASSWORD` - SMTP password (never logged)
- `EMAIL_FROM` - Sender address
- `EMAIL_TO` - Comma-separated recipient list
- `EMAIL_CONTRACT_FILTER` - Optional contract ID filter

### 8. Tests (`tests/email_notification_tests.rs`)

Created comprehensive integration tests:
- Contract filtering logic
- Empty filter behavior (accepts all events)
- Channel closure handling
- Configuration parsing
- Email address parsing with whitespace handling
- Contract filter parsing

### 9. Documentation

Created detailed documentation:

#### `docs/email-notifications.md`
- Feature overview
- Configuration instructions
- Examples for Gmail, SendGrid, and AWS SES
- Email format and batching behavior
- Troubleshooting guide
- Security considerations
- Comparison with webhooks

#### Updated `README.md`
- Added "Notifications" section
- Documented email notification feature
- Added `soroban_pulse_email_failures_total` metric

#### Updated `CHANGELOG.md`
- Added email notification feature to [Unreleased] section
- Listed all new configuration variables
- Mentioned new Prometheus metric

## Acceptance Criteria ✅

All acceptance criteria from the issue have been met:

- ✅ Email notifications are sent when `EMAIL_SMTP_HOST` is configured
- ✅ Notifications are batched with a maximum of one email per minute
- ✅ The email body includes a summary of new events with contract ID, type, and ledger
- ✅ `EMAIL_CONTRACT_FILTER` limits notifications to specific contracts
- ✅ SMTP credentials are never logged (handled in config and email modules)
- ✅ The `.env.example` documents all email configuration variables
- ✅ Tests verify email batching and contract filtering logic

## Architecture

```
┌─────────────────┐
│   Indexer       │
│   (indexer.rs)  │
└────────┬────────┘
         │
         │ broadcasts events
         ▼
┌─────────────────────────────────┐
│  Broadcast Channel              │
│  (tokio::sync::broadcast)       │
└────┬────────────────────────┬───┘
     │                        │
     │                        │
     ▼                        ▼
┌─────────────┐      ┌──────────────────┐
│  Webhook    │      │  Email Notifier  │
│  Delivery   │      │  (email.rs)      │
└─────────────┘      └──────────────────┘
                              │
                              │ batches for 60s
                              ▼
                     ┌──────────────────┐
                     │  SMTP Server     │
                     │  (lettre)        │
                     └──────────────────┘
```

## Security Considerations

1. **Credential Protection**: SMTP passwords are never logged or exposed in metrics
2. **TLS Support**: Uses STARTTLS on port 587 for encrypted connections
3. **Environment Variables**: Credentials loaded from environment, not hardcoded
4. **Input Validation**: Email addresses and contract IDs are validated
5. **Rate Limiting**: Batching prevents email flooding (max 1 email/minute)

## Testing

The implementation includes:
- Unit tests for email notifier creation and configuration
- Integration tests for contract filtering
- Tests for channel handling (lag, closure)
- Configuration parsing tests
- Email address parsing with edge cases

To run the tests:
```bash
cargo test email
```

## Future Enhancements

Potential improvements for future iterations:
1. HTML email templates with better formatting
2. Configurable batch interval (currently fixed at 60 seconds)
3. Email templates with custom branding
4. Attachment support for detailed event logs
5. Per-contract email routing (different recipients for different contracts)
6. Email digest scheduling (daily/weekly summaries)

## Migration Guide

To enable email notifications on an existing deployment:

1. Set the required environment variables:
   ```bash
   EMAIL_SMTP_HOST=smtp.example.com
   EMAIL_SMTP_PORT=587
   EMAIL_SMTP_USER=your-email@example.com
   EMAIL_SMTP_PASSWORD=your-password
   EMAIL_FROM=soroban-pulse@example.com
   EMAIL_TO=admin@example.com,alerts@example.com
   ```

2. (Optional) Configure contract filtering:
   ```bash
   EMAIL_CONTRACT_FILTER=CONTRACT_ID_1,CONTRACT_ID_2
   ```

3. Restart the service

4. Monitor the `soroban_pulse_email_failures_total` metric for delivery issues

## Related Issues

This implementation addresses the email notification feature request and complements the existing webhook delivery feature for HTTP-based notifications.
