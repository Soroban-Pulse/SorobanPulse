# Error Handling Documentation

## Overview

SorobanPulse uses a unified error handling system centered around the [`AppError`] enum.
All handlers return `Result<T, AppError>`, and domain errors convert automatically via `From`.

## Standardized Error Types

| Variant | HTTP Status | Code | Description |
|---------|-------------|------|-------------|
| `NotFound` | 404 | NOT_FOUND | Resource not found |
| `Validation(msg)` | 400 | VALIDATION_ERROR | Invalid input provided |
| `ValidationWithDetails(msg, errors)` | 400 | VALIDATION_ERROR | Validation with field details |
| `Forbidden(msg)` | 403 | FORBIDDEN | Access denied |
| `Auth(AuthError::Unauthenticated)` | 401 | UNAUTHORIZED | Missing or invalid credentials |
| `Auth(AuthError::TokenExpired)` | 401 | UNAUTHORIZED | Authentication token expired |
| `Database(sqlx::Error)` | 500 | DATABASE_ERROR | Database operation failed |
| `DatabaseDomain(DatabaseError::NotFound)` | 404 | NOT_FOUND | Record not found in database |
| `DatabaseDomain(DatabaseError::Timeout)` | 503 | DATABASE_TIMEOUT | Query timed out |
| `DatabaseDomain(DatabaseError::PoolExhausted)` | 503 | DATABASE_POOL_EXHAUSTED | Connection pool exhausted |
| `Rpc(RpcError::RateLimited)` | 429 | UPSTREAM_RATE_LIMITED | Upstream rate limit exceeded |
| `Indexer(IndexerError::LockNotAcquired)` | 503 | INDEXER_LOCK_NOT_ACQUIRED | Advisory lock not acquired |
| `Indexer(IndexerError::Stalled)` | 503 | INDEXER_STALLED | Indexer has stalled |
| `Webhook(WebhookError::InvalidEndpoint)` | 400 | INVALID_WEBHOOK_ENDPOINT | Invalid webhook URL |
| `Webhook(WebhookError::SignatureMismatch)` | 401 | WEBHOOK_SIGNATURE_MISMATCH | HMAC signature mismatch |
| `Subscription(SubscriptionError::NotFound)` | 404 | NOT_FOUND | Subscription not found |
| `Subscription(SubscriptionError::LimitExceeded)` | 429 | SUBSCRIPTION_LIMIT_EXCEEDED | Subscription limit exceeded |

## Standardized Constructors

Use the convenience constructors instead of manually building enum variants:

```rust
use crate::error::AppError;

// Instead of: AppError::Validation("bad input".to_string())
return Err(AppError::validation("bad input"));

// Instead of: AppError::Internal(format!("..."))
return Err(AppError::internal("something went wrong"));

// Instead of: AppError::NotFound
return Err(AppError::not_found());

// Instead of: AppError::Forbidden("denied".to_string())
return Err(AppError::forbidden("denied"));

// Instead of: AppError::Auth(AuthError::Unauthenticated)
return Err(AppError::unauthorized());

// Instead of: AppError::Subscription(SubscriptionError::LimitExceeded)
return Err(AppError::rate_limited());
```

## Error Context

Attach structured metadata to any error using [`ErrorContext`]:

```rust
use crate::error::{AppError, ErrorContext};

Err(AppError::not_found().with_context(
    ErrorContext::new()
        .with_operation("get_event")
        .with_entity("event:abc"),
)?)
```

## Error Recovery

Use [`ErrorRecovery`] to provide actionable guidance to users:

```rust
use crate::error::ErrorRecovery;

let recovery = ErrorRecovery::new("Check your API key and try again")
    .with_doc("https://docs.example.com/api-keys");
```

## Error Documentation Generator

Generate this documentation programmatically using `error::generate_error_docs()`.
