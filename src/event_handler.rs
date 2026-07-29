//! # Event handler trait — Issue #665
//!
//! Defines an extensible processing pipeline for Soroban events.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                     HandlerPipeline                       │
//! │  ┌─────────────┐  ┌────────────┐  ┌──────────────────┐  │
//! │  │ Validation  │→ │  Filter    │→ │   Transform      │→ … │
//! │  │ Decorator   │  │ Decorator  │  │   Decorator      │   │
//! │  └─────────────┘  └────────────┘  └──────────────────┘  │
//! │                                        ↓                  │
//! │                              ┌──────────────────┐        │
//! │                              │ Base / Sink       │        │
//! │                              │ EventHandler      │        │
//! │                              └──────────────────┘        │
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Core trait
//!
//! [`EventHandler`] is the single, async trait that every stage implements.
//!
//! ```rust,ignore
//! #[async_trait]
//! impl EventHandler for MyHandler {
//!     async fn handle(&self, event: Event) -> Result<HandlerResult, AppError> {
//!         // process event
//!         Ok(HandlerResult::Processed(event))
//!     }
//!
//!     fn name(&self) -> &str { "my_handler" }
//! }
//! ```
//!
//! ## Composition
//!
//! Use the decorator types to wrap any [`EventHandler`]:
//!
//! ```rust,ignore
//! let pipeline = ValidationDecorator::new(inner_handler, |e| {
//!     if e.ledger == 0 { Err(AppError::Validation("ledger 0 invalid".into())) }
//!     else { Ok(()) }
//! });
//! let pipeline = FilterDecorator::new(pipeline, |e| e.event_type == "contract");
//! let pipeline = TransformDecorator::new(pipeline, |mut e| {
//!     e.event_type = e.event_type.to_uppercase();
//!     e
//! });
//! ```
//!
//! Or use the [`HandlerPipeline`] builder:
//!
//! ```rust,ignore
//! let pipeline = HandlerPipeline::new(sink)
//!     .validate(|e| { /* ... */ Ok(()) })
//!     .filter(|e| e.ledger > 1_000_000)
//!     .transform(|e| { /* normalise */ e });
//! ```
//!
//! ## Extension Points
//! - Implement [`EventHandler`] for any new processing stage.
//! - Use [`RecoveryDecorator`] to add error-recovery logic without modifying
//!   the wrapped handler.
//! - Chain multiple handlers via [`ComposedHandler`].

use async_trait::async_trait;
use std::sync::Arc;

use crate::{error::AppError, models::Event};

// ---------------------------------------------------------------------------
// Core result type
// ---------------------------------------------------------------------------

/// The outcome of a single handler stage.
#[derive(Debug, Clone)]
pub enum HandlerResult {
    /// The event was processed and (possibly transformed) output is ready for
    /// the next stage.
    Processed(Event),

    /// The event was intentionally skipped by this stage (e.g. filtered out).
    /// Subsequent stages will not receive the event.
    Skipped,

    /// The event has been fully consumed by this stage and should not
    /// propagate further.
    Consumed,
}

// ---------------------------------------------------------------------------
// Core trait
// ---------------------------------------------------------------------------

/// The primary trait for event processing stages.
///
/// Every middleware, decorator, and sink in the pipeline implements this trait.
///
/// ## Implementing
/// - `handle` receives an owned [`Event`]; the implementation may mutate and
///   return it as [`HandlerResult::Processed`], discard it as
///   [`HandlerResult::Skipped`], or signal it was consumed.
/// - Return `Err(AppError)` only for unrecoverable failures.  Wrap the handler
///   in a [`RecoveryDecorator`] to provide fallback behaviour.
/// - `name` is used in logs and metrics; override it with a meaningful label.
#[async_trait]
pub trait EventHandler: Send + Sync {
    /// Process a single event.
    async fn handle(&self, event: Event) -> Result<HandlerResult, AppError>;

    /// Human-readable name for this handler stage (used in logs/metrics).
    fn name(&self) -> &str {
        "event_handler"
    }

    /// Optional description of what this stage does.
    fn description(&self) -> Option<&str> {
        None
    }
}

// ---------------------------------------------------------------------------
// No-op / passthrough handler (useful as a base/sink in tests)
// ---------------------------------------------------------------------------

/// A trivial handler that passes every event through unchanged.
///
/// Useful as the final sink in a pipeline when you only care about the
/// decorators applied above it.
pub struct PassthroughHandler;

#[async_trait]
impl EventHandler for PassthroughHandler {
    async fn handle(&self, event: Event) -> Result<HandlerResult, AppError> {
        Ok(HandlerResult::Processed(event))
    }

    fn name(&self) -> &str {
        "passthrough"
    }

    fn description(&self) -> Option<&str> {
        Some("Passes every event through unchanged.")
    }
}

// ---------------------------------------------------------------------------
// Filter decorator
// ---------------------------------------------------------------------------

/// Wraps an inner handler with a predicate filter.
///
/// Events that do not satisfy the predicate are returned as
/// [`HandlerResult::Skipped`] — the inner handler never sees them.
///
/// ## Usage
/// ```rust,ignore
/// let handler = FilterDecorator::new(inner, |e: &Event| e.ledger > 1_000_000);
/// ```
pub struct FilterDecorator<H, F>
where
    H: EventHandler,
    F: Fn(&Event) -> bool + Send + Sync,
{
    inner: H,
    predicate: F,
    handler_name: String,
}

impl<H, F> FilterDecorator<H, F>
where
    H: EventHandler,
    F: Fn(&Event) -> bool + Send + Sync,
{
    /// Create a new filter wrapping `inner` with the given `predicate`.
    pub fn new(inner: H, predicate: F) -> Self {
        let handler_name = format!("filter({})", inner.name());
        Self {
            inner,
            predicate,
            handler_name,
        }
    }
}

#[async_trait]
impl<H, F> EventHandler for FilterDecorator<H, F>
where
    H: EventHandler,
    F: Fn(&Event) -> bool + Send + Sync,
{
    async fn handle(&self, event: Event) -> Result<HandlerResult, AppError> {
        if (self.predicate)(&event) {
            self.inner.handle(event).await
        } else {
            Ok(HandlerResult::Skipped)
        }
    }

    fn name(&self) -> &str {
        &self.handler_name
    }

    fn description(&self) -> Option<&str> {
        Some("Filters events based on a predicate; non-matching events are skipped.")
    }
}

// ---------------------------------------------------------------------------
// Transform decorator
// ---------------------------------------------------------------------------

/// Wraps an inner handler with a synchronous transformation function.
///
/// The transform is applied to the event *before* it is passed to the inner
/// handler, allowing normalisation, enrichment, or field rewriting.
///
/// ## Usage
/// ```rust,ignore
/// let handler = TransformDecorator::new(inner, |mut e: Event| {
///     e.event_type = e.event_type.to_lowercase();
///     e
/// });
/// ```
pub struct TransformDecorator<H, T>
where
    H: EventHandler,
    T: Fn(Event) -> Event + Send + Sync,
{
    inner: H,
    transform: T,
    handler_name: String,
}

impl<H, T> TransformDecorator<H, T>
where
    H: EventHandler,
    T: Fn(Event) -> Event + Send + Sync,
{
    /// Create a new transform wrapping `inner` with `transform`.
    pub fn new(inner: H, transform: T) -> Self {
        let handler_name = format!("transform({})", inner.name());
        Self {
            inner,
            transform,
            handler_name,
        }
    }
}

#[async_trait]
impl<H, T> EventHandler for TransformDecorator<H, T>
where
    H: EventHandler,
    T: Fn(Event) -> Event + Send + Sync,
{
    async fn handle(&self, event: Event) -> Result<HandlerResult, AppError> {
        let transformed = (self.transform)(event);
        self.inner.handle(transformed).await
    }

    fn name(&self) -> &str {
        &self.handler_name
    }

    fn description(&self) -> Option<&str> {
        Some("Applies a synchronous transformation to each event before passing it on.")
    }
}

// ---------------------------------------------------------------------------
// Validation decorator
// ---------------------------------------------------------------------------

/// Wraps an inner handler with a validation function.
///
/// If the validator returns `Err`, the error is propagated and the inner
/// handler is **not** called.  This makes it easy to enforce invariants at the
/// boundary of a processing stage.
///
/// ## Usage
/// ```rust,ignore
/// let handler = ValidationDecorator::new(inner, |e: &Event| {
///     if e.ledger == 0 {
///         Err(AppError::Validation("ledger 0 is invalid".into()))
///     } else {
///         Ok(())
///     }
/// });
/// ```
pub struct ValidationDecorator<H, V>
where
    H: EventHandler,
    V: Fn(&Event) -> Result<(), AppError> + Send + Sync,
{
    inner: H,
    validator: V,
    handler_name: String,
}

impl<H, V> ValidationDecorator<H, V>
where
    H: EventHandler,
    V: Fn(&Event) -> Result<(), AppError> + Send + Sync,
{
    /// Create a new validation decorator wrapping `inner` with `validator`.
    pub fn new(inner: H, validator: V) -> Self {
        let handler_name = format!("validate({})", inner.name());
        Self {
            inner,
            validator,
            handler_name,
        }
    }
}

#[async_trait]
impl<H, V> EventHandler for ValidationDecorator<H, V>
where
    H: EventHandler,
    V: Fn(&Event) -> Result<(), AppError> + Send + Sync,
{
    async fn handle(&self, event: Event) -> Result<HandlerResult, AppError> {
        (self.validator)(&event)?;
        self.inner.handle(event).await
    }

    fn name(&self) -> &str {
        &self.handler_name
    }

    fn description(&self) -> Option<&str> {
        Some("Validates each event before passing it to the inner handler.")
    }
}

// ---------------------------------------------------------------------------
// Recovery decorator
// ---------------------------------------------------------------------------

/// The action to take when an inner handler returns an error.
pub enum RecoveryAction {
    /// Skip the event and log the error — processing continues.
    Skip,
    /// Consume the event — processing continues without the event.
    Consume,
    /// Re-raise the error — propagates to the caller.
    Propagate,
}

/// Wraps an inner handler with error-recovery logic.
///
/// When the inner handler returns an `Err`, the `recovery_fn` is called to
/// decide whether to skip, consume, or re-raise.
///
/// ## Usage
/// ```rust,ignore
/// let handler = RecoveryDecorator::new(inner, |err| {
///     tracing::warn!(error = %err, "event processing failed; skipping");
///     RecoveryAction::Skip
/// });
/// ```
pub struct RecoveryDecorator<H, R>
where
    H: EventHandler,
    R: Fn(AppError) -> RecoveryAction + Send + Sync,
{
    inner: H,
    recovery_fn: R,
    handler_name: String,
}

impl<H, R> RecoveryDecorator<H, R>
where
    H: EventHandler,
    R: Fn(AppError) -> RecoveryAction + Send + Sync,
{
    /// Create a new recovery decorator wrapping `inner` with `recovery_fn`.
    pub fn new(inner: H, recovery_fn: R) -> Self {
        let handler_name = format!("recover({})", inner.name());
        Self {
            inner,
            recovery_fn,
            handler_name,
        }
    }
}

#[async_trait]
impl<H, R> EventHandler for RecoveryDecorator<H, R>
where
    H: EventHandler,
    R: Fn(AppError) -> RecoveryAction + Send + Sync,
{
    async fn handle(&self, event: Event) -> Result<HandlerResult, AppError> {
        match self.inner.handle(event).await {
            Ok(result) => Ok(result),
            Err(e) => match (self.recovery_fn)(e) {
                RecoveryAction::Skip => Ok(HandlerResult::Skipped),
                RecoveryAction::Consume => Ok(HandlerResult::Consumed),
                RecoveryAction::Propagate => {
                    Err(AppError::Internal("handler error propagated".into()))
                }
            },
        }
    }

    fn name(&self) -> &str {
        &self.handler_name
    }

    fn description(&self) -> Option<&str> {
        Some("Provides error recovery for the wrapped handler.")
    }
}

// ---------------------------------------------------------------------------
// Composed handler (fan-out)
// ---------------------------------------------------------------------------

/// A handler that forwards each event to a list of inner handlers in sequence.
///
/// Processing stops at the first handler that returns [`HandlerResult::Skipped`]
/// or [`HandlerResult::Consumed`], or the first that returns `Err`.
pub struct ComposedHandler {
    handlers: Vec<Arc<dyn EventHandler>>,
    handler_name: String,
}

impl ComposedHandler {
    /// Create a composed handler from a list of inner handlers.
    pub fn new(handlers: Vec<Arc<dyn EventHandler>>) -> Self {
        let names: Vec<&str> = handlers.iter().map(|h| h.name()).collect();
        let handler_name = format!("composed({})", names.join(" → "));
        Self {
            handlers,
            handler_name,
        }
    }
}

#[async_trait]
impl EventHandler for ComposedHandler {
    async fn handle(&self, event: Event) -> Result<HandlerResult, AppError> {
        let mut current = event;

        for handler in &self.handlers {
            match handler.handle(current).await? {
                HandlerResult::Processed(e) => {
                    current = e;
                }
                HandlerResult::Skipped => return Ok(HandlerResult::Skipped),
                HandlerResult::Consumed => return Ok(HandlerResult::Consumed),
            }
        }

        Ok(HandlerResult::Processed(current))
    }

    fn name(&self) -> &str {
        &self.handler_name
    }

    fn description(&self) -> Option<&str> {
        Some("Forwards events through a sequence of handlers.")
    }
}

// ---------------------------------------------------------------------------
// Pipeline builder
// ---------------------------------------------------------------------------

/// Fluent builder for assembling a handler pipeline.
///
/// Decorators are applied in the order they are added.  The inner-most stage
/// is always `sink` (the handler passed to [`HandlerPipeline::new`]).
///
/// ## Ordering
/// The pipeline executes decorators in the order they were added via the
/// builder methods, wrapping from the outside in:
///
/// ```text
/// validate → filter → transform → … → sink
/// ```
///
/// ## Usage
/// ```rust,ignore
/// let pipeline: Box<dyn EventHandler> = HandlerPipeline::new(PassthroughHandler)
///     .validate(|e| if e.ledger > 0 { Ok(()) } else { Err(AppError::Validation("ledger must be > 0".into())) })
///     .filter(|e: &Event| e.event_type == "contract")
///     .transform(|mut e: Event| { e.event_type = e.event_type.to_lowercase(); e })
///     .build();
/// ```
pub struct HandlerPipeline {
    inner: Box<dyn EventHandler>,
}

impl HandlerPipeline {
    /// Start a new pipeline with the given sink handler as the innermost stage.
    pub fn new<H: EventHandler + 'static>(sink: H) -> Self {
        Self {
            inner: Box::new(sink),
        }
    }

    /// Add a validation step.  The validator receives a shared reference to
    /// the event; returning `Err` short-circuits the pipeline.
    #[must_use]
    pub fn validate<V>(self, validator: V) -> Self
    where
        V: Fn(&Event) -> Result<(), AppError> + Send + Sync + 'static,
    {
        Self {
            inner: Box::new(ValidationDecorator {
                handler_name: format!("validate({})", self.inner.name()),
                inner: self.inner,
                validator,
            }),
        }
    }

    /// Add a filter step.  Events that do not match the predicate are skipped.
    #[must_use]
    pub fn filter<F>(self, predicate: F) -> Self
    where
        F: Fn(&Event) -> bool + Send + Sync + 'static,
    {
        Self {
            inner: Box::new(FilterDecorator {
                handler_name: format!("filter({})", self.inner.name()),
                inner: self.inner,
                predicate,
            }),
        }
    }

    /// Add a transform step.
    #[must_use]
    pub fn transform<T>(self, transform: T) -> Self
    where
        T: Fn(Event) -> Event + Send + Sync + 'static,
    {
        Self {
            inner: Box::new(TransformDecorator {
                handler_name: format!("transform({})", self.inner.name()),
                inner: self.inner,
                transform,
            }),
        }
    }

    /// Add error recovery.
    #[must_use]
    pub fn recover<R>(self, recovery_fn: R) -> Self
    where
        R: Fn(AppError) -> RecoveryAction + Send + Sync + 'static,
    {
        Self {
            inner: Box::new(RecoveryDecorator {
                handler_name: format!("recover({})", self.inner.name()),
                inner: self.inner,
                recovery_fn,
            }),
        }
    }

    /// Finalise the pipeline, returning a boxed [`EventHandler`].
    pub fn build(self) -> Box<dyn EventHandler> {
        self.inner
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Event;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_event(ledger: i64, event_type: &str) -> Event {
        Event {
            id: Uuid::new_v4(),
            contract_id: "CABC1234567890123456789012345678901234567890123456".to_string(),
            event_type: event_type.to_string(),
            tx_hash: "abc123".to_string(),
            ledger,
            timestamp: Utc::now(),
            event_data: serde_json::json!({}),
            created_at: Utc::now(),
        }
    }

    // -----------------------------------------------------------------------
    // PassthroughHandler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn passthrough_processes_event() {
        let handler = PassthroughHandler;
        let event = make_event(100, "contract");
        let result = handler.handle(event).await.unwrap();
        assert!(matches!(result, HandlerResult::Processed(_)));
    }

    // -----------------------------------------------------------------------
    // FilterDecorator
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn filter_passes_matching_event() {
        let handler = FilterDecorator::new(PassthroughHandler, |e: &Event| e.ledger > 50);
        let result = handler.handle(make_event(100, "contract")).await.unwrap();
        assert!(matches!(result, HandlerResult::Processed(_)));
    }

    #[tokio::test]
    async fn filter_skips_non_matching_event() {
        let handler = FilterDecorator::new(PassthroughHandler, |e: &Event| e.ledger > 200);
        let result = handler.handle(make_event(100, "contract")).await.unwrap();
        assert!(matches!(result, HandlerResult::Skipped));
    }

    // -----------------------------------------------------------------------
    // TransformDecorator
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn transform_modifies_event() {
        let handler = TransformDecorator::new(PassthroughHandler, |mut e: Event| {
            e.event_type = "TRANSFORMED".to_string();
            e
        });
        let result = handler.handle(make_event(1, "original")).await.unwrap();
        if let HandlerResult::Processed(e) = result {
            assert_eq!(e.event_type, "TRANSFORMED");
        } else {
            panic!("expected Processed");
        }
    }

    // -----------------------------------------------------------------------
    // ValidationDecorator
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn validation_passes_valid_event() {
        let handler = ValidationDecorator::new(PassthroughHandler, |e: &Event| {
            if e.ledger > 0 {
                Ok(())
            } else {
                Err(AppError::Validation("ledger must be > 0".into()))
            }
        });
        let result = handler.handle(make_event(1, "contract")).await.unwrap();
        assert!(matches!(result, HandlerResult::Processed(_)));
    }

    #[tokio::test]
    async fn validation_rejects_invalid_event() {
        let handler = ValidationDecorator::new(PassthroughHandler, |e: &Event| {
            if e.ledger > 0 {
                Ok(())
            } else {
                Err(AppError::Validation("ledger must be > 0".into()))
            }
        });
        let result = handler.handle(make_event(0, "contract")).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // RecoveryDecorator
    // -----------------------------------------------------------------------

    struct AlwaysFailHandler;

    #[async_trait]
    impl EventHandler for AlwaysFailHandler {
        async fn handle(&self, _event: Event) -> Result<HandlerResult, AppError> {
            Err(AppError::Internal("intentional failure".into()))
        }

        fn name(&self) -> &str {
            "always_fail"
        }
    }

    #[tokio::test]
    async fn recovery_skip_returns_skipped() {
        let handler =
            RecoveryDecorator::new(AlwaysFailHandler, |_err| RecoveryAction::Skip);
        let result = handler.handle(make_event(1, "contract")).await.unwrap();
        assert!(matches!(result, HandlerResult::Skipped));
    }

    #[tokio::test]
    async fn recovery_consume_returns_consumed() {
        let handler =
            RecoveryDecorator::new(AlwaysFailHandler, |_err| RecoveryAction::Consume);
        let result = handler.handle(make_event(1, "contract")).await.unwrap();
        assert!(matches!(result, HandlerResult::Consumed));
    }

    #[tokio::test]
    async fn recovery_propagate_returns_err() {
        let handler =
            RecoveryDecorator::new(AlwaysFailHandler, |_err| RecoveryAction::Propagate);
        let result = handler.handle(make_event(1, "contract")).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // ComposedHandler
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn composed_handler_processes_through_all_stages() {
        let h1: Arc<dyn EventHandler> = Arc::new(PassthroughHandler);
        let h2: Arc<dyn EventHandler> = Arc::new(PassthroughHandler);
        let composed = ComposedHandler::new(vec![h1, h2]);
        let result = composed.handle(make_event(1, "contract")).await.unwrap();
        assert!(matches!(result, HandlerResult::Processed(_)));
    }

    #[tokio::test]
    async fn composed_handler_stops_at_first_skip() {
        struct SkipHandler;
        #[async_trait]
        impl EventHandler for SkipHandler {
            async fn handle(&self, _e: Event) -> Result<HandlerResult, AppError> {
                Ok(HandlerResult::Skipped)
            }
            fn name(&self) -> &str {
                "skip"
            }
        }

        let h1: Arc<dyn EventHandler> = Arc::new(SkipHandler);
        let h2: Arc<dyn EventHandler> = Arc::new(PassthroughHandler);
        let composed = ComposedHandler::new(vec![h1, h2]);
        let result = composed.handle(make_event(1, "contract")).await.unwrap();
        assert!(matches!(result, HandlerResult::Skipped));
    }

    // -----------------------------------------------------------------------
    // HandlerPipeline builder
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn pipeline_builder_validate_filter_transform() {
        let pipeline = HandlerPipeline::new(PassthroughHandler)
            .validate(|e: &Event| {
                if e.ledger > 0 {
                    Ok(())
                } else {
                    Err(AppError::Validation("ledger must be > 0".into()))
                }
            })
            .filter(|e: &Event| e.event_type == "contract")
            .transform(|mut e: Event| {
                e.event_type = "transformed".to_string();
                e
            })
            .build();

        // Valid contract event → processed and transformed.
        let result = pipeline.handle(make_event(1, "contract")).await.unwrap();
        if let HandlerResult::Processed(e) = result {
            assert_eq!(e.event_type, "transformed");
        } else {
            panic!("expected Processed");
        }

        // Diagnostic event → filtered out.
        let result2 = pipeline.handle(make_event(1, "diagnostic")).await.unwrap();
        assert!(matches!(result2, HandlerResult::Skipped));
    }

    #[tokio::test]
    async fn pipeline_builder_validation_rejects_bad_event() {
        let pipeline = HandlerPipeline::new(PassthroughHandler)
            .validate(|e: &Event| {
                if e.ledger > 0 {
                    Ok(())
                } else {
                    Err(AppError::Validation("ledger must be > 0".into()))
                }
            })
            .build();

        let result = pipeline.handle(make_event(0, "contract")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn pipeline_handler_name_reflects_decorators() {
        let pipeline = HandlerPipeline::new(PassthroughHandler)
            .filter(|_: &Event| true)
            .build();

        // Name should include "filter(" and "passthrough".
        assert!(pipeline.name().contains("passthrough"));
        assert!(pipeline.name().contains("filter"));
    }

    #[tokio::test]
    async fn pipeline_with_recovery() {
        let pipeline = HandlerPipeline::new(PassthroughHandler)
            .recover(|_err| RecoveryAction::Skip)
            .build();

        let result = pipeline.handle(make_event(1, "contract")).await.unwrap();
        assert!(matches!(result, HandlerResult::Processed(_)));
    }
}
