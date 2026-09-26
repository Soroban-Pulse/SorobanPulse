//! # Error handling module — Issue #662
//!
//! Consolidates all application error types into a single, consistent module.
//!
//! ## Design
//! - [`AppError`] is the top-level unified error enum used across all handlers.
//! - Domain-specific error sub-types ([`DatabaseError`], [`IndexerError`],
//!   [`WebhookError`], [`SubscriptionError`], [`AuthError`], [`RpcError`])
//!   convert automatically into [`AppError`] via `From` implementations.
//! - [`ErrorContext`] attaches structured metadata (operation, entity, request_id)
//!   to any error for richer log output and API responses.
//! - [`ErrorResponse`] is the machine-readable JSON body returned to clients.
//!
//! ## Extension Points
//! Add a new domain error by:
//! 1. Declaring a variant in the appropriate domain enum (or adding a new enum).
//! 2. Implementing `From<YourDomainError> for AppError`.
//! 3. Adding a `(StatusCode, &'static str)` arm inside `AppError::status_and_code`.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::cell::RefCell;
use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Request-scoped correlation ID (thread-local)
// ---------------------------------------------------------------------------

thread_local! {
    static REQUEST_ID: RefCell<Option<String>> = RefCell::new(None);
}

/// Store a request-scoped correlation ID so that all errors produced during
/// a handler invocation can reference it automatically.
pub fn set_request_id(id: String) {
    REQUEST_ID.with(|rid| {
        *rid.borrow_mut() = Some(id);
    });
}

/// Retrieve the current request-scoped correlation ID, generating a fresh UUID
/// if none has been set.
pub fn get_request_id() -> String {
    REQUEST_ID.with(|rid| {
        rid.borrow()
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string())
    })
}

// ---------------------------------------------------------------------------
// Structured API response body
// ---------------------------------------------------------------------------

/// Machine-readable error response returned in the `application/json` body.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    /// Human-readable description of what went wrong.
    pub error: String,
    /// Machine-readable error code (e.g. `"VALIDATION_ERROR"`).
    pub code: &'static str,
    /// Correlation ID that ties this response to a specific request in logs.
    pub correlation_id: String,
    /// Optional operation context (which logical action failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// Optional entity context (which resource was involved).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    /// Field-level validation details (only present for `VALIDATION_ERROR`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_errors: Option<Vec<ValidationErrorDetail>>,
}

/// Details of a single field-level validation failure.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ValidationErrorDetail {
    /// JSON Pointer to the offending value (e.g. `"/page"`).
    pub instance_path: String,
    /// The schema constraint that was violated.
    pub schema_path: String,
    /// Human-readable explanation.
    pub message: String,
}

// ---------------------------------------------------------------------------
// Error context — attach structured metadata to errors
// ---------------------------------------------------------------------------

/// Metadata that can be attached to an [`AppError`] to provide richer
/// diagnostics in logs and API responses.
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    /// The logical operation that was being attempted (e.g. `"fetch_events"`).
    pub operation: Option<String>,
    /// The entity being operated on (e.g. `"contract:CABC..."`).
    pub entity: Option<String>,
    /// Overrides the auto-generated correlation ID when set explicitly.
    pub request_id: Option<String>,
}

impl ErrorContext {
    /// Create a new empty context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the operation name.
    pub fn with_operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }

    /// Set the entity description.
    pub fn with_entity(mut self, entity: impl Into<String>) -> Self {
        self.entity = Some(entity.into());
        self
    }

    /// Override the correlation / request ID.
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Domain-specific error types
// ---------------------------------------------------------------------------

/// Errors originating from database operations.
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Query failed: {0}")]
    QueryFailed(#[from] sqlx::Error),

    #[error("Query timed out")]
    Timeout,

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Unique constraint violation: {0}")]
    UniqueViolation(String),

    #[error("Connection pool exhausted")]
    PoolExhausted,
}

/// Errors originating from the background indexer.
#[derive(Debug, Error)]
pub enum IndexerError {
    #[error("RPC call failed: {0}")]
    RpcCallFailed(String),

    #[error("Ledger gap detected: expected {expected}, got {got}")]
    LedgerGap { expected: u64, got: u64 },

    #[error("Advisory lock not acquired")]
    LockNotAcquired,

    #[error("Indexer stalled: last ledger {last_ledger}")]
    Stalled { last_ledger: u64 },
}

/// Errors originating from webhook delivery.
#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("Delivery failed after {attempts} attempts: {reason}")]
    DeliveryFailed { attempts: u32, reason: String },

    #[error("Invalid endpoint URL: {0}")]
    InvalidEndpoint(String),

    #[error("HMAC signature mismatch")]
    SignatureMismatch,
}

/// Errors originating from subscription management.
#[derive(Debug, Error)]
pub enum SubscriptionError {
    #[error("Subscription not found: {0}")]
    NotFound(String),

    #[error("Invalid filter: {0}")]
    InvalidFilter(String),

    #[error("Subscription limit exceeded")]
    LimitExceeded,
}

/// Authentication and authorisation errors.
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Missing or invalid API key")]
    Unauthenticated,

    #[error("Insufficient privileges: {0}")]
    Forbidden(String),

    #[error("Token expired")]
    TokenExpired,
}

/// Errors from external RPC / network calls.
#[derive(Debug, Error)]
pub enum RpcError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Unexpected response: {0}")]
    UnexpectedResponse(String),

    #[error("Rate limited by upstream")]
    RateLimited,
}

// ---------------------------------------------------------------------------
// Unified AppError
// ---------------------------------------------------------------------------

/// The single unified error type used throughout the application.
///
/// Every handler and service function returns `Result<T, AppError>`.
/// Domain errors convert automatically via their `From` implementations.
///
/// ## Adding a new error kind
/// 1. Add a variant here (or to an existing domain enum above).
/// 2. Add a match arm in [`AppError::status_and_code`].
/// 3. If it wraps a domain enum, add a `From` impl below.
#[derive(Debug, Error)]
pub enum AppError {
    // --- database domain --------------------------------------------------
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Database error: {0}")]
    DatabaseDomain(DatabaseError),

    // --- network / RPC domain ---------------------------------------------
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("RPC error: {0}")]
    Rpc(RpcError),

    // --- indexer domain ---------------------------------------------------
    #[error("Indexer error: {0}")]
    Indexer(IndexerError),

    // --- webhook domain ---------------------------------------------------
    #[error("Webhook error: {0}")]
    Webhook(WebhookError),

    // --- subscription domain ----------------------------------------------
    #[error("Subscription error: {0}")]
    Subscription(SubscriptionError),

    // --- auth domain ------------------------------------------------------
    #[error("Authentication error: {0}")]
    Auth(AuthError),

    // --- generic / cross-cutting ------------------------------------------
    #[error("Not found")]
    NotFound,

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Validation error with details")]
    ValidationWithDetails(String, Vec<ValidationErrorDetail>),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Internal error: {0}")]
    #[allow(dead_code)]
    Internal(String),

    // --- errors with attached context ------------------------------------
    /// An error bundled with rich context metadata.  Use
    /// [`AppError::with_context`] to construct these.
    #[error("{source}: op={op:?} entity={entity:?}")]
    WithContext {
        source: Box<AppError>,
        op: Option<String>,
        entity: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Domain-error → AppError conversions
// ---------------------------------------------------------------------------

impl From<DatabaseError> for AppError {
    fn from(e: DatabaseError) -> Self {
        AppError::DatabaseDomain(e)
    }
}

impl From<RpcError> for AppError {
    fn from(e: RpcError) -> Self {
        AppError::Rpc(e)
    }
}

impl From<IndexerError> for AppError {
    fn from(e: IndexerError) -> Self {
        AppError::Indexer(e)
    }
}

impl From<WebhookError> for AppError {
    fn from(e: WebhookError) -> Self {
        AppError::Webhook(e)
    }
}

impl From<SubscriptionError> for AppError {
    fn from(e: SubscriptionError) -> Self {
        AppError::Subscription(e)
    }
}

impl From<AuthError> for AppError {
    fn from(e: AuthError) -> Self {
        AppError::Auth(e)
    }
}

// ---------------------------------------------------------------------------
// Context attachment
// ---------------------------------------------------------------------------

impl AppError {
    /// Attach structured context metadata to this error.
    ///
    /// ```rust,ignore
    /// return Err(AppError::NotFound.with_context(
    ///     ErrorContext::new()
    ///         .with_operation("get_event")
    ///         .with_entity(format!("event:{}", id)),
    /// ));
    /// ```
    pub fn with_context(self, ctx: ErrorContext) -> Self {
        AppError::WithContext {
            source: Box::new(self),
            op: ctx.operation,
            entity: ctx.entity,
        }
    }

    // -----------------------------------------------------------------------
    // Status code + machine-readable code mapping
    // -----------------------------------------------------------------------

    /// Returns the HTTP status code and machine-readable error code string for
    /// this error.  This is the single authoritative mapping — add new variants
    /// here when extending the error taxonomy.
    pub fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND"),

            AppError::Validation(_) | AppError::ValidationWithDetails(_, _) => {
                (StatusCode::BAD_REQUEST, "VALIDATION_ERROR")
            }

            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),

            AppError::Auth(AuthError::Unauthenticated) | AppError::Auth(AuthError::TokenExpired) => {
                (StatusCode::UNAUTHORIZED, "UNAUTHORIZED")
            }
            AppError::Auth(AuthError::Forbidden(_)) => (StatusCode::FORBIDDEN, "FORBIDDEN"),

            AppError::Database(e) if is_query_timeout_sqlx(e) => {
                (StatusCode::SERVICE_UNAVAILABLE, "DATABASE_TIMEOUT")
            }
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR"),

            AppError::DatabaseDomain(DatabaseError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, "NOT_FOUND")
            }
            AppError::DatabaseDomain(DatabaseError::Timeout) => {
                (StatusCode::SERVICE_UNAVAILABLE, "DATABASE_TIMEOUT")
            }
            AppError::DatabaseDomain(DatabaseError::UniqueViolation(_)) => {
                (StatusCode::CONFLICT, "CONFLICT")
            }
            AppError::DatabaseDomain(DatabaseError::PoolExhausted) => {
                (StatusCode::SERVICE_UNAVAILABLE, "DATABASE_POOL_EXHAUSTED")
            }
            AppError::DatabaseDomain(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR"),

            AppError::Rpc(RpcError::RateLimited) => {
                (StatusCode::TOO_MANY_REQUESTS, "UPSTREAM_RATE_LIMITED")
            }
            AppError::Rpc(_) | AppError::Http(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR")
            }

            AppError::Indexer(IndexerError::LockNotAcquired) => {
                (StatusCode::SERVICE_UNAVAILABLE, "INDEXER_LOCK_NOT_ACQUIRED")
            }
            AppError::Indexer(IndexerError::Stalled { .. }) => {
                (StatusCode::SERVICE_UNAVAILABLE, "INDEXER_STALLED")
            }
            AppError::Indexer(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INDEXER_ERROR"),

            AppError::Webhook(WebhookError::InvalidEndpoint(_)) => {
                (StatusCode::BAD_REQUEST, "INVALID_WEBHOOK_ENDPOINT")
            }
            AppError::Webhook(WebhookError::SignatureMismatch) => {
                (StatusCode::UNAUTHORIZED, "WEBHOOK_SIGNATURE_MISMATCH")
            }
            AppError::Webhook(_) => (StatusCode::INTERNAL_SERVER_ERROR, "WEBHOOK_ERROR"),

            AppError::Subscription(SubscriptionError::NotFound(_)) => {
                (StatusCode::NOT_FOUND, "NOT_FOUND")
            }
            AppError::Subscription(SubscriptionError::InvalidFilter(_)) => {
                (StatusCode::BAD_REQUEST, "INVALID_SUBSCRIPTION_FILTER")
            }
            AppError::Subscription(SubscriptionError::LimitExceeded) => {
                (StatusCode::TOO_MANY_REQUESTS, "SUBSCRIPTION_LIMIT_EXCEEDED")
            }

            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),

            AppError::WithContext { source, .. } => source.status_and_code(),
        }
    }

    /// Build the full [`ErrorResponse`] for this error.
    fn build_response(&self) -> (StatusCode, ErrorResponse) {
        let (status, code) = self.status_and_code();
        let correlation_id = get_request_id();

        // Extract context metadata from WithContext wrapper if present.
        let (operation, entity) = if let AppError::WithContext { op, entity, .. } = self {
            (op.clone(), entity.clone())
        } else {
            (None, None)
        };

        // Unwrap through context to reach the underlying error for the message.
        let inner = self.inner();

        let (message, validation_errors) = match inner {
            AppError::Validation(msg) => (msg.clone(), None),
            AppError::ValidationWithDetails(msg, errors) => (msg.clone(), Some(errors.clone())),
            AppError::Forbidden(msg) => (msg.clone(), None),
            AppError::Auth(AuthError::Forbidden(msg)) => (msg.clone(), None),
            AppError::DatabaseDomain(DatabaseError::NotFound(msg)) => (msg.clone(), None),
            AppError::DatabaseDomain(DatabaseError::UniqueViolation(msg)) => (msg.clone(), None),
            AppError::Subscription(SubscriptionError::NotFound(msg)) => (msg.clone(), None),
            AppError::Subscription(SubscriptionError::InvalidFilter(msg)) => (msg.clone(), None),
            AppError::Webhook(WebhookError::InvalidEndpoint(msg)) => (msg.clone(), None),
            AppError::Database(e) if is_query_timeout_sqlx(e) => {
                ("query timeout".to_string(), None)
            }
            AppError::Database(e) => {
                tracing::error!(error = %e, "Database error");
                ("internal server error".to_string(), None)
            }
            AppError::DatabaseDomain(e) => {
                tracing::error!(error = %e, "Database domain error");
                ("internal server error".to_string(), None)
            }
            AppError::Http(e) => {
                tracing::error!(error = %e, "HTTP error");
                ("internal server error".to_string(), None)
            }
            AppError::Rpc(e) => {
                tracing::error!(error = %e, "RPC error");
                ("internal server error".to_string(), None)
            }
            AppError::Indexer(e) => {
                tracing::error!(error = %e, "Indexer error");
                (e.to_string(), None)
            }
            AppError::Webhook(e) => {
                tracing::error!(error = %e, "Webhook error");
                ("webhook error".to_string(), None)
            }
            AppError::Internal(msg) => {
                tracing::error!(error = %msg, "Internal error");
                ("internal server error".to_string(), None)
            }
            AppError::NotFound => ("not found".to_string(), None),
            AppError::Auth(AuthError::Unauthenticated) => ("unauthorized".to_string(), None),
            AppError::Auth(AuthError::TokenExpired) => ("token expired".to_string(), None),
            _ => ("internal server error".to_string(), None),
        };

        let body = ErrorResponse {
            error: message,
            code,
            correlation_id,
            operation,
            entity,
            validation_errors,
        };

        (status, body)
    }

    /// Unwrap through any `WithContext` wrappers to reach the base error.
    fn inner(&self) -> &AppError {
        match self {
            AppError::WithContext { source, .. } => source.inner(),
            other => other,
        }
    }

    /// Convenience method: produce `(StatusCode, Json<ErrorResponse>)` tuple.
    pub fn into_response_parts(self) -> (StatusCode, Json<serde_json::Value>) {
        let (status, body) = self.build_response();
        let value = serde_json::to_value(&body).unwrap_or_else(|_| {
            serde_json::json!({
                "error": "internal server error",
                "code": "INTERNAL_ERROR",
                "correlation_id": get_request_id(),
            })
        });
        (status, Json(value))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = self.build_response();
        (status, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` when the sqlx error represents a Postgres query timeout
/// (error code 57014 = `query_canceled`, which covers `statement_timeout`).
fn is_query_timeout_sqlx(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = e {
        return db_err.code().as_deref() == Some("57014");
    }
    false
}

/// Convenience trait that adds `.with_context(ctx)` to `Result<T, AppError>`.
///
/// ```rust,ignore
/// use crate::error::{ErrorContext, ResultExt};
/// pool.fetch_one(&query)
///     .await
///     .map_err(AppError::from)
///     .with_error_context(ErrorContext::new().with_operation("get_event"))?;
/// ```
pub trait ResultExt<T> {
    fn with_error_context(self, ctx: ErrorContext) -> Result<T, AppError>;
}

impl<T> ResultExt<T> for Result<T, AppError> {
    fn with_error_context(self, ctx: ErrorContext) -> Result<T, AppError> {
        self.map_err(|e| e.with_context(ctx))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn not_found_maps_to_404() {
        assert_eq!(
            AppError::NotFound.status_and_code(),
            (StatusCode::NOT_FOUND, "NOT_FOUND")
        );
    }

    #[test]
    fn validation_maps_to_400() {
        assert_eq!(
            AppError::Validation("bad input".into()).status_and_code(),
            (StatusCode::BAD_REQUEST, "VALIDATION_ERROR")
        );
    }

    #[test]
    fn forbidden_maps_to_403() {
        assert_eq!(
            AppError::Forbidden("nope".into()).status_and_code(),
            (StatusCode::FORBIDDEN, "FORBIDDEN")
        );
    }

    #[test]
    fn internal_maps_to_500() {
        assert_eq!(
            AppError::Internal("oops".into()).status_and_code(),
            (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR")
        );
    }

    #[test]
    fn domain_db_not_found_maps_to_404() {
        let e: AppError = DatabaseError::NotFound("event:123".into()).into();
        assert_eq!(e.status_and_code(), (StatusCode::NOT_FOUND, "NOT_FOUND"));
    }

    #[test]
    fn domain_db_timeout_maps_to_503() {
        let e: AppError = DatabaseError::Timeout.into();
        assert_eq!(
            e.status_and_code(),
            (StatusCode::SERVICE_UNAVAILABLE, "DATABASE_TIMEOUT")
        );
    }

    #[test]
    fn auth_unauthenticated_maps_to_401() {
        let e: AppError = AuthError::Unauthenticated.into();
        assert_eq!(
            e.status_and_code(),
            (StatusCode::UNAUTHORIZED, "UNAUTHORIZED")
        );
    }

    #[test]
    fn context_wrapping_preserves_status() {
        let e = AppError::NotFound.with_context(
            ErrorContext::new()
                .with_operation("fetch_event")
                .with_entity("event:abc"),
        );
        assert_eq!(e.status_and_code(), (StatusCode::NOT_FOUND, "NOT_FOUND"));
    }

    #[test]
    fn result_ext_works() {
        let result: Result<(), AppError> = Err(AppError::NotFound);
        let wrapped = result.with_error_context(
            ErrorContext::new().with_operation("test_op"),
        );
        assert!(matches!(wrapped, Err(AppError::WithContext { .. })));
    }

    #[test]
    fn subscription_limit_maps_to_429() {
        let e: AppError = SubscriptionError::LimitExceeded.into();
        assert_eq!(
            e.status_and_code(),
            (StatusCode::TOO_MANY_REQUESTS, "SUBSCRIPTION_LIMIT_EXCEEDED")
        );
    }

    #[test]
    fn rpc_rate_limited_maps_to_429() {
        let e = AppError::Rpc(RpcError::RateLimited);
        assert_eq!(
            e.status_and_code(),
            (StatusCode::TOO_MANY_REQUESTS, "UPSTREAM_RATE_LIMITED")
        );
    }

    #[test]
    fn webhook_signature_mismatch_maps_to_401() {
        let e: AppError = WebhookError::SignatureMismatch.into();
        assert_eq!(
            e.status_and_code(),
            (StatusCode::UNAUTHORIZED, "WEBHOOK_SIGNATURE_MISMATCH")
        );
    }

    #[test]
    fn set_and_get_request_id() {
        set_request_id("test-id-123".to_string());
        assert_eq!(get_request_id(), "test-id-123");
        // Clean up
        REQUEST_ID.with(|r| *r.borrow_mut() = None);
    }
}
