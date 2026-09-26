//! Streaming query results (Issue #960).
//!
//! The response layer in [`crate::streaming_response`] solves half the problem:
//! it writes a large result set to the client without holding the whole
//! response in memory. That is worth nothing if the query itself buffered every
//! row before the first byte went out, which is what `fetch_all` does.
//!
//! This module is the other half. It pulls rows from the database in bounded
//! batches behind a keyset cursor, and hands them out one at a time as a
//! [`crate::streaming_response::StreamingResultIterator`]. Peak memory is one
//! batch, whatever the size of the result set.
//!
//! Keyset rather than `OFFSET`: `OFFSET n` makes the database walk and discard
//! `n` rows on every batch, so paging through a large result set costs O(n²)
//! overall and drifts when rows are inserted mid-stream. A cursor carrying the
//! last key seen costs the same for batch one and batch ten thousand, and never
//! skips or repeats a row because something was inserted behind it.
//!
//! ## What this module owns
//!
//! * **Batching** — [`StreamingQueryConfig::batch_size`] rows per round trip.
//! * **Cursors** — [`Cursored`] gives each row its key; [`QueryCursor`] carries
//!   the position between batches.
//! * **Error handling** — a batch failure either ends the stream or is retried
//!   past, bounded by a consecutive-error budget, per [`StreamErrorPolicy`].
//! * **Keep-alive** — a tick emitted when a batch is slow, so an idle
//!   connection is not reaped mid-query.
//! * **Progress and cancellation** — a [`QueryStreamHandle`] shared with the
//!   caller.

use futures::stream::{Stream, StreamExt};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

use crate::streaming_response::StreamingResultIterator;

/// Rows requested per round trip when nothing else is configured.
pub const DEFAULT_BATCH_SIZE: i64 = 500;

/// Largest batch size accepted. A batch is held in memory whole, so an
/// unbounded value would reintroduce exactly the buffering this module exists
/// to avoid.
pub const MAX_BATCH_SIZE: i64 = 10_000;

/// Seconds without a completed batch before a keep-alive tick is emitted.
pub const DEFAULT_KEEPALIVE_SECS: u64 = 15;

/// Consecutive failing batches tolerated under [`StreamErrorPolicy::SkipBatch`].
pub const DEFAULT_MAX_CONSECUTIVE_ERRORS: u32 = 3;

/// A row that can locate itself, so the next batch can resume after it.
///
/// The key must be unique and ordered the same way the query orders rows —
/// usually the primary key, or the `(timestamp, id)` pair the query sorts by
/// rendered as a single sortable string. A key that does not match the query's
/// ordering silently skips or repeats rows.
pub trait Cursored {
    fn cursor_key(&self) -> String;
}

/// Position within a result set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryCursor {
    /// Key of the last row handed out; `None` before the first batch.
    pub key: Option<String>,
    pub batches_fetched: u64,
    pub rows_emitted: u64,
    pub exhausted: bool,
}

impl QueryCursor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resume from a key a client supplied, for a paged API that hands the
    /// cursor back to the caller between requests.
    pub fn resume_from(key: impl Into<String>) -> Self {
        Self {
            key: Some(key.into()),
            ..Self::default()
        }
    }

    fn advance(&mut self, last_key: Option<String>, rows: u64) {
        if last_key.is_some() {
            self.key = last_key;
        }
        self.batches_fetched += 1;
        self.rows_emitted += rows;
    }
}

/// What to do when a batch fetch fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamErrorPolicy {
    /// End the stream on the first error, after surfacing it to the client.
    ///
    /// The default. A half-delivered result set that looks complete is worse
    /// than one that visibly failed.
    #[default]
    FailFast,
    /// Surface the error and retry the same cursor position, up to
    /// [`StreamingQueryConfig::max_consecutive_errors`] times in a row.
    ///
    /// For long exports where a transient blip should not discard an hour of
    /// delivered rows. The budget is what stops a permanently broken query from
    /// spinning forever.
    SkipBatch,
}

/// Tunables for one streamed query.
#[derive(Debug, Clone, Copy)]
pub struct StreamingQueryConfig {
    pub batch_size: i64,
    /// Stop after this many batches. `None` streams to exhaustion.
    pub max_batches: Option<u64>,
    pub keepalive_interval: Duration,
    pub error_policy: StreamErrorPolicy,
    pub max_consecutive_errors: u32,
}

impl Default for StreamingQueryConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_BATCH_SIZE,
            max_batches: None,
            keepalive_interval: Duration::from_secs(DEFAULT_KEEPALIVE_SECS),
            error_policy: StreamErrorPolicy::FailFast,
            max_consecutive_errors: DEFAULT_MAX_CONSECUTIVE_ERRORS,
        }
    }
}

impl StreamingQueryConfig {
    /// Read overrides from the environment.
    ///
    /// An unparseable or out-of-range value falls back to the default rather
    /// than failing startup: a malformed batch size should not take the service
    /// down, and the clamped value is logged.
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Some(batch_size) = parse_env::<i64>("QUERY_STREAM_BATCH_SIZE") {
            config = config.with_batch_size(batch_size);
        }
        if let Some(max_batches) = parse_env::<u64>("QUERY_STREAM_MAX_BATCHES") {
            config.max_batches = (max_batches > 0).then_some(max_batches);
        }
        if let Some(secs) = parse_env::<u64>("QUERY_STREAM_KEEPALIVE_SECS") {
            config.keepalive_interval = Duration::from_secs(secs.max(1));
        }
        if let Some(max_errors) = parse_env::<u32>("QUERY_STREAM_MAX_CONSECUTIVE_ERRORS") {
            config.max_consecutive_errors = max_errors;
        }

        config
    }

    /// Set the batch size, clamped to `1..=MAX_BATCH_SIZE`.
    pub fn with_batch_size(mut self, batch_size: i64) -> Self {
        let clamped = batch_size.clamp(1, MAX_BATCH_SIZE);
        if clamped != batch_size {
            warn!(
                requested = batch_size,
                applied = clamped,
                "Query stream batch size clamped"
            );
        }
        self.batch_size = clamped;
        self
    }

    pub fn with_max_batches(mut self, max_batches: Option<u64>) -> Self {
        self.max_batches = max_batches.filter(|n| *n > 0);
        self
    }

    pub fn with_keepalive_interval(mut self, interval: Duration) -> Self {
        self.keepalive_interval = interval;
        self
    }

    pub fn with_error_policy(mut self, policy: StreamErrorPolicy) -> Self {
        self.error_policy = policy;
        self
    }

    pub fn with_max_consecutive_errors(mut self, max: u32) -> Self {
        self.max_consecutive_errors = max;
        self
    }
}

fn parse_env<T: std::str::FromStr>(var: &str) -> Option<T> {
    let raw = std::env::var(var).ok()?;
    match raw.trim().parse::<T>() {
        Ok(value) => Some(value),
        Err(_) => {
            warn!(var, raw, "Ignoring unparseable query streaming setting");
            None
        }
    }
}

/// One thing coming out of a streamed query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamItem<T> {
    Row(T),
    /// The query has produced nothing for [`StreamingQueryConfig::keepalive_interval`].
    ///
    /// Carried through so a transport that can express idleness (NDJSON, SSE)
    /// sends something and keeps proxies and load balancers from reaping the
    /// connection. A JSON array body has nowhere to put it, so
    /// [`QueryStream::into_rows`] drops it.
    KeepAlive,
}

/// Bytes for a keep-alive in an NDJSON stream: a blank line, which every
/// line-delimited JSON reader skips.
pub const NDJSON_KEEPALIVE: &[u8] = b"\n";

/// Bytes for a keep-alive in an SSE stream: a comment frame, which the
/// EventSource spec requires clients to ignore.
pub const SSE_KEEPALIVE: &[u8] = b": keep-alive\n\n";

/// Live counters for a streamed query.
#[derive(Debug, Default)]
pub struct QueryStreamProgress {
    batches_fetched: AtomicU64,
    rows_emitted: AtomicU64,
    keepalives_sent: AtomicU64,
    batch_errors: AtomicU64,
    completed: AtomicBool,
    cancelled: AtomicBool,
    exhausted: AtomicBool,
}

/// A point-in-time copy of a query stream's counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueryStreamStats {
    pub batches_fetched: u64,
    pub rows_emitted: u64,
    pub keepalives_sent: u64,
    pub batch_errors: u64,
    pub completed: bool,
    pub cancelled: bool,
    /// True when the stream ended because the query ran out of rows, as opposed
    /// to hitting `max_batches`, an error, or a cancellation.
    pub exhausted: bool,
}

impl QueryStreamStats {
    /// Mean rows per round trip, 0.0 before the first batch.
    pub fn avg_batch_size(&self) -> f64 {
        if self.batches_fetched == 0 {
            0.0
        } else {
            self.rows_emitted as f64 / self.batches_fetched as f64
        }
    }

    /// True when the stream ended without delivering the whole result set.
    pub fn is_partial(&self) -> bool {
        self.completed && !self.exhausted
    }
}

/// Caller-side control of a streamed query.
#[derive(Debug, Clone)]
pub struct QueryStreamHandle {
    progress: Arc<QueryStreamProgress>,
    cancel: Arc<AtomicBool>,
}

impl QueryStreamHandle {
    fn new() -> Self {
        Self {
            progress: Arc::new(QueryStreamProgress::default()),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Stop the stream. The in-flight batch finishes first; no further batch is
    /// requested, which is the part that costs.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    pub fn stats(&self) -> QueryStreamStats {
        QueryStreamStats {
            batches_fetched: self.progress.batches_fetched.load(Ordering::Relaxed),
            rows_emitted: self.progress.rows_emitted.load(Ordering::Relaxed),
            keepalives_sent: self.progress.keepalives_sent.load(Ordering::Relaxed),
            batch_errors: self.progress.batch_errors.load(Ordering::Relaxed),
            completed: self.progress.completed.load(Ordering::Acquire),
            cancelled: self.progress.cancelled.load(Ordering::Acquire),
            exhausted: self.progress.exhausted.load(Ordering::Acquire),
        }
    }
}

/// A batch fetch: given the cursor position and a row limit, return the next
/// rows in the query's order.
///
/// Implementations run the actual SQL. Keeping it a closure rather than a
/// concrete query type is what lets the same batching, cursor, error, keep-alive
/// and cancellation logic sit in front of any query in the codebase.
pub type BatchFuture<T> =
    Pin<Box<dyn Future<Output = Result<Vec<T>, sqlx::Error>> + Send + 'static>>;

pub type BatchFetch<T> = Arc<dyn Fn(Option<String>, i64) -> BatchFuture<T> + Send + Sync + 'static>;

/// A query being consumed in batches.
pub struct QueryStream<T> {
    fetch: BatchFetch<T>,
    config: StreamingQueryConfig,
    cursor: QueryCursor,
    handle: QueryStreamHandle,
}

impl<T> QueryStream<T>
where
    T: Cursored + Send + 'static,
{
    pub fn new(fetch: BatchFetch<T>) -> Self {
        Self::with_config(fetch, StreamingQueryConfig::default())
    }

    pub fn with_config(fetch: BatchFetch<T>, config: StreamingQueryConfig) -> Self {
        Self {
            fetch,
            config,
            cursor: QueryCursor::new(),
            handle: QueryStreamHandle::new(),
        }
    }

    /// Start from a caller-supplied cursor position.
    pub fn starting_at(mut self, cursor: QueryCursor) -> Self {
        self.cursor = cursor;
        self
    }

    pub fn config(&self) -> StreamingQueryConfig {
        self.config
    }

    pub fn cursor(&self) -> &QueryCursor {
        &self.cursor
    }

    /// Handle for watching progress or cancelling. Take it before the stream is
    /// consumed.
    pub fn handle(&self) -> QueryStreamHandle {
        self.handle.clone()
    }

    /// Rows and keep-alive ticks, in order.
    ///
    /// Built with `unfold` over the cursor state: each step either returns a
    /// buffered row, fetches the next batch, or ends the stream.
    pub fn into_stream(self) -> Pin<Box<dyn Stream<Item = Result<StreamItem<T>, sqlx::Error>> + Send>>
    {
        let QueryStream {
            fetch,
            config,
            cursor,
            handle,
        } = self;

        struct State<T> {
            fetch: BatchFetch<T>,
            config: StreamingQueryConfig,
            cursor: QueryCursor,
            handle: QueryStreamHandle,
            buffered: std::collections::VecDeque<T>,
            /// A batch that has not finished yet, parked across keep-alive
            /// ticks so a slow query keeps making progress instead of being
            /// restarted every tick.
            in_flight: Option<BatchFuture<T>>,
            consecutive_errors: u32,
            finished: bool,
        }

        let state = State {
            fetch,
            config,
            cursor,
            handle,
            buffered: std::collections::VecDeque::new(),
            in_flight: None,
            consecutive_errors: 0,
            finished: false,
        };

        Box::pin(futures::stream::unfold(state, |mut state| async move {
            loop {
                if state.finished {
                    return None;
                }

                // Hand out what the last batch produced before asking for more.
                if let Some(row) = state.buffered.pop_front() {
                    state
                        .handle
                        .progress
                        .rows_emitted
                        .fetch_add(1, Ordering::Relaxed);
                    crate::metrics::record_query_stream_row();
                    return Some((Ok(StreamItem::Row(row)), state));
                }

                if state.handle.is_cancelled() {
                    state.handle.progress.cancelled.store(true, Ordering::Release);
                    state.handle.progress.completed.store(true, Ordering::Release);
                    debug!(
                        rows = state.cursor.rows_emitted,
                        "Query stream cancelled by caller"
                    );
                    crate::metrics::record_query_stream_cancelled();
                    return None;
                }

                if state.cursor.exhausted {
                    state.handle.progress.exhausted.store(true, Ordering::Release);
                    state.handle.progress.completed.store(true, Ordering::Release);
                    crate::metrics::record_query_stream_completed(state.cursor.rows_emitted);
                    return None;
                }

                if let Some(max) = state.config.max_batches {
                    if state.cursor.batches_fetched >= max {
                        debug!(max, "Query stream stopped at max_batches");
                        state.handle.progress.completed.store(true, Ordering::Release);
                        crate::metrics::record_query_stream_truncated();
                        return None;
                    }
                }

                // Resume the parked batch if there is one, otherwise start the
                // next. `in_flight` is the reason a keep-alive is free: the
                // future is moved back into the state rather than dropped, so a
                // query slower than the keep-alive interval still finishes
                // instead of being cancelled and restarted on every tick.
                let mut in_flight = match state.in_flight.take() {
                    Some(future) => future,
                    None => {
                        let fetch = Arc::clone(&state.fetch);
                        let key = state.cursor.key.clone();
                        fetch(key, state.config.batch_size)
                    }
                };

                let batch = match tokio::time::timeout(
                    state.config.keepalive_interval,
                    &mut in_flight,
                )
                .await
                {
                    Ok(batch) => batch,
                    Err(_) => {
                        state.in_flight = Some(in_flight);
                        state
                            .handle
                            .progress
                            .keepalives_sent
                            .fetch_add(1, Ordering::Relaxed);
                        crate::metrics::record_query_stream_keepalive();
                        return Some((Ok(StreamItem::KeepAlive), state));
                    }
                };

                match batch {
                    Ok(rows) => {
                        state.consecutive_errors = 0;
                        let fetched = rows.len();
                        let last_key = rows.last().map(Cursored::cursor_key);

                        state.cursor.advance(last_key, fetched as u64);
                        state
                            .handle
                            .progress
                            .batches_fetched
                            .fetch_add(1, Ordering::Relaxed);
                        crate::metrics::record_query_stream_batch(fetched as u64);

                        // A short batch means the query has no more rows. Asking
                        // again would cost a round trip to learn nothing.
                        if (fetched as i64) < state.config.batch_size {
                            state.cursor.exhausted = true;
                        }

                        state.buffered.extend(rows);
                    }
                    Err(e) => {
                        state.consecutive_errors += 1;
                        state
                            .handle
                            .progress
                            .batch_errors
                            .fetch_add(1, Ordering::Relaxed);
                        crate::metrics::record_query_stream_error();

                        let give_up = match state.config.error_policy {
                            StreamErrorPolicy::FailFast => true,
                            StreamErrorPolicy::SkipBatch => {
                                state.consecutive_errors > state.config.max_consecutive_errors
                            }
                        };

                        warn!(
                            error = %e,
                            consecutive = state.consecutive_errors,
                            give_up,
                            "Query stream batch failed"
                        );

                        // The error goes to the client either way. Ending the
                        // stream silently would hand back a truncated result set
                        // that looks complete.
                        state.finished = give_up;
                        if give_up {
                            state.handle.progress.completed.store(true, Ordering::Release);
                        }
                        return Some((Err(e), state));
                    }
                }
            }
        }))
    }

    /// Rows only, with keep-alive ticks dropped.
    ///
    /// This is what feeds [`crate::streaming_response`]: a JSON array body has
    /// nowhere to put a keep-alive frame. Use [`QueryStream::into_stream`] for a
    /// transport that can carry one.
    pub fn into_rows(self) -> StreamingResultIterator<T> {
        Box::pin(self.into_stream().filter_map(|item| async move {
            match item {
                Ok(StreamItem::Row(row)) => Some(Ok(row)),
                Ok(StreamItem::KeepAlive) => None,
                Err(e) => Some(Err(e)),
            }
        }))
    }
}

/// Build a batch fetcher from an async closure.
///
/// ```rust,ignore
/// let fetch = batch_fetcher(move |cursor, limit| {
///     let pool = pool.clone();
///     async move {
///         sqlx::query_as::<_, Event>(
///             "SELECT * FROM events WHERE ($1::text IS NULL OR id > $1) \
///              ORDER BY id LIMIT $2",
///         )
///         .bind(cursor)
///         .bind(limit)
///         .fetch_all(&pool)
///         .await
///     }
/// });
/// ```
pub fn batch_fetcher<T, F, Fut>(f: F) -> BatchFetch<T>
where
    F: Fn(Option<String>, i64) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Vec<T>, sqlx::Error>> + Send + 'static,
    T: Send + 'static,
{
    Arc::new(move |cursor, limit| Box::pin(f(cursor, limit)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Row {
        id: u64,
    }

    impl Cursored for Row {
        fn cursor_key(&self) -> String {
            self.id.to_string()
        }
    }

    /// A fetcher over an in-memory table, so cursor and batching behaviour is
    /// exercised without a database.
    fn table_fetcher(total: u64, calls: Arc<AtomicUsize>) -> BatchFetch<Row> {
        batch_fetcher(move |cursor: Option<String>, limit: i64| {
            let calls = Arc::clone(&calls);
            async move {
                calls.fetch_add(1, Ordering::Relaxed);
                let after: u64 = cursor.and_then(|c| c.parse().ok()).map_or(0, |id| id + 1);
                let rows = (after..total)
                    .take(usize::try_from(limit).unwrap_or(0))
                    .map(|id| Row { id })
                    .collect::<Vec<_>>();
                Ok(rows)
            }
        })
    }

    async fn collect_rows(stream: StreamingResultIterator<Row>) -> Vec<Row> {
        stream
            .filter_map(|r| async move { r.ok() })
            .collect::<Vec<_>>()
            .await
    }

    // ── Configuration ────────────────────────────────────────────────────────

    #[test]
    fn default_config_is_bounded() {
        let config = StreamingQueryConfig::default();
        assert_eq!(config.batch_size, DEFAULT_BATCH_SIZE);
        assert_eq!(config.max_batches, None);
        assert_eq!(config.error_policy, StreamErrorPolicy::FailFast);
        assert_eq!(
            config.keepalive_interval,
            Duration::from_secs(DEFAULT_KEEPALIVE_SECS)
        );
    }

    #[test]
    fn batch_size_is_clamped_to_a_usable_range() {
        assert_eq!(
            StreamingQueryConfig::default().with_batch_size(0).batch_size,
            1
        );
        assert_eq!(
            StreamingQueryConfig::default()
                .with_batch_size(-10)
                .batch_size,
            1
        );
        assert_eq!(
            StreamingQueryConfig::default()
                .with_batch_size(i64::MAX)
                .batch_size,
            MAX_BATCH_SIZE
        );
        assert_eq!(
            StreamingQueryConfig::default().with_batch_size(250).batch_size,
            250
        );
    }

    #[test]
    fn zero_max_batches_means_unlimited_rather_than_nothing() {
        assert_eq!(
            StreamingQueryConfig::default()
                .with_max_batches(Some(0))
                .max_batches,
            None
        );
        assert_eq!(
            StreamingQueryConfig::default()
                .with_max_batches(Some(3))
                .max_batches,
            Some(3)
        );
    }

    // ── Cursor ───────────────────────────────────────────────────────────────

    #[test]
    fn a_fresh_cursor_starts_before_the_first_row() {
        let cursor = QueryCursor::new();
        assert_eq!(cursor.key, None);
        assert_eq!(cursor.rows_emitted, 0);
        assert!(!cursor.exhausted);
    }

    #[test]
    fn a_cursor_can_resume_from_a_supplied_key() {
        let cursor = QueryCursor::resume_from("abc");
        assert_eq!(cursor.key, Some("abc".to_string()));
        assert_eq!(cursor.batches_fetched, 0);
    }

    #[test]
    fn advancing_keeps_the_last_key_when_a_batch_is_empty() {
        let mut cursor = QueryCursor::resume_from("k1");
        cursor.advance(Some("k2".to_string()), 5);
        assert_eq!(cursor.key, Some("k2".to_string()));

        // An empty batch must not reset the position back to the start.
        cursor.advance(None, 0);
        assert_eq!(cursor.key, Some("k2".to_string()));
        assert_eq!(cursor.batches_fetched, 2);
        assert_eq!(cursor.rows_emitted, 5);
    }

    // ── Batching ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn streams_every_row_in_order_across_batches() {
        let calls = Arc::new(AtomicUsize::new(0));
        let stream = QueryStream::with_config(
            table_fetcher(25, Arc::clone(&calls)),
            StreamingQueryConfig::default().with_batch_size(10),
        );
        let handle = stream.handle();

        let rows = collect_rows(stream.into_rows()).await;

        assert_eq!(rows.len(), 25);
        assert_eq!(rows.first(), Some(&Row { id: 0 }));
        assert_eq!(rows.last(), Some(&Row { id: 24 }));
        assert!(rows.windows(2).all(|w| w[0].id < w[1].id));

        let stats = handle.stats();
        assert_eq!(stats.rows_emitted, 25);
        assert_eq!(stats.batches_fetched, 3);
        assert!(stats.exhausted);
    }

    #[tokio::test]
    async fn a_short_batch_ends_the_stream_without_an_extra_round_trip() {
        let calls = Arc::new(AtomicUsize::new(0));
        let stream = QueryStream::with_config(
            table_fetcher(15, Arc::clone(&calls)),
            StreamingQueryConfig::default().with_batch_size(10),
        );

        let rows = collect_rows(stream.into_rows()).await;

        assert_eq!(rows.len(), 15);
        // Two fetches: a full batch of 10, then a short batch of 5 that proves
        // exhaustion. A third fetch would be a round trip to learn nothing.
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn an_empty_result_set_streams_nothing_and_terminates() {
        let calls = Arc::new(AtomicUsize::new(0));
        let stream = QueryStream::new(table_fetcher(0, Arc::clone(&calls)));
        let handle = stream.handle();

        assert!(collect_rows(stream.into_rows()).await.is_empty());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(handle.stats().exhausted);
    }

    #[tokio::test]
    async fn a_batch_size_of_one_still_walks_the_whole_table() {
        let calls = Arc::new(AtomicUsize::new(0));
        let stream = QueryStream::with_config(
            table_fetcher(5, Arc::clone(&calls)),
            StreamingQueryConfig::default().with_batch_size(1),
        );

        let rows = collect_rows(stream.into_rows()).await;
        assert_eq!(rows.len(), 5);
    }

    #[tokio::test]
    async fn max_batches_truncates_the_stream() {
        let calls = Arc::new(AtomicUsize::new(0));
        let stream = QueryStream::with_config(
            table_fetcher(1_000, Arc::clone(&calls)),
            StreamingQueryConfig::default()
                .with_batch_size(10)
                .with_max_batches(Some(3)),
        );
        let handle = stream.handle();

        let rows = collect_rows(stream.into_rows()).await;

        assert_eq!(rows.len(), 30);
        let stats = handle.stats();
        assert_eq!(stats.batches_fetched, 3);
        assert!(!stats.exhausted);
        assert!(stats.is_partial());
    }

    #[tokio::test]
    async fn resuming_from_a_cursor_skips_what_was_already_delivered() {
        let calls = Arc::new(AtomicUsize::new(0));
        let stream = QueryStream::with_config(
            table_fetcher(20, Arc::clone(&calls)),
            StreamingQueryConfig::default().with_batch_size(5),
        )
        .starting_at(QueryCursor::resume_from("9"));

        let rows = collect_rows(stream.into_rows()).await;

        assert_eq!(rows.len(), 10);
        assert_eq!(rows.first(), Some(&Row { id: 10 }));
    }

    // ── Error handling ───────────────────────────────────────────────────────

    /// Fails the first `failures` fetches, then serves the table.
    fn flaky_fetcher(total: u64, failures: usize) -> BatchFetch<Row> {
        let attempts = Arc::new(AtomicUsize::new(0));
        batch_fetcher(move |cursor: Option<String>, limit: i64| {
            let attempts = Arc::clone(&attempts);
            async move {
                if attempts.fetch_add(1, Ordering::Relaxed) < failures {
                    return Err(sqlx::Error::PoolClosed);
                }
                let after: u64 = cursor.and_then(|c| c.parse().ok()).map_or(0, |id| id + 1);
                Ok((after..total)
                    .take(usize::try_from(limit).unwrap_or(0))
                    .map(|id| Row { id })
                    .collect())
            }
        })
    }

    #[tokio::test]
    async fn fail_fast_surfaces_the_error_and_stops() {
        let stream = QueryStream::with_config(
            flaky_fetcher(50, 1),
            StreamingQueryConfig::default()
                .with_batch_size(10)
                .with_error_policy(StreamErrorPolicy::FailFast),
        );
        let handle = stream.handle();

        let items: Vec<_> = stream.into_stream().collect().await;

        assert_eq!(items.len(), 1);
        assert!(items[0].is_err(), "the error must reach the client");
        assert_eq!(handle.stats().batch_errors, 1);
        assert!(!handle.stats().exhausted);
    }

    #[tokio::test]
    async fn skip_batch_retries_past_a_transient_failure() {
        let stream = QueryStream::with_config(
            flaky_fetcher(20, 2),
            StreamingQueryConfig::default()
                .with_batch_size(10)
                .with_error_policy(StreamErrorPolicy::SkipBatch)
                .with_max_consecutive_errors(3),
        );
        let handle = stream.handle();

        let rows = collect_rows(stream.into_rows()).await;

        assert_eq!(rows.len(), 20, "delivered rows should survive a blip");
        assert_eq!(handle.stats().batch_errors, 2);
    }

    #[tokio::test]
    async fn skip_batch_gives_up_once_the_error_budget_is_spent() {
        // Never recovers, so the budget is the only thing that stops it.
        let stream = QueryStream::with_config(
            flaky_fetcher(20, usize::MAX),
            StreamingQueryConfig::default()
                .with_error_policy(StreamErrorPolicy::SkipBatch)
                .with_max_consecutive_errors(2),
        );
        let handle = stream.handle();

        let items: Vec<_> = stream.into_stream().collect().await;

        assert!(items.iter().all(Result::is_err));
        assert_eq!(items.len(), 3, "budget of 2 allows a third, failing attempt");
        assert_eq!(handle.stats().batch_errors, 3);
    }

    // ── Keep-alive ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_slow_batch_emits_a_keepalive_rather_than_going_silent() {
        let fetch: BatchFetch<Row> = batch_fetcher(move |_cursor, _limit| async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            Ok(vec![Row { id: 1 }])
        });

        let stream = QueryStream::with_config(
            fetch,
            StreamingQueryConfig::default()
                .with_batch_size(10)
                .with_keepalive_interval(Duration::from_millis(20)),
        );
        let handle = stream.handle();

        let items: Vec<_> = stream.into_stream().take(3).collect().await;

        assert!(
            items
                .iter()
                .any(|item| matches!(item, Ok(StreamItem::KeepAlive))),
            "a slow batch should tick rather than leave the connection silent"
        );
        assert!(handle.stats().keepalives_sent > 0);
    }

    #[tokio::test]
    async fn keepalives_are_dropped_from_the_row_stream() {
        let fetch: BatchFetch<Row> = batch_fetcher(move |cursor: Option<String>, _limit| async move {
            if cursor.is_some() {
                return Ok(vec![]);
            }
            tokio::time::sleep(Duration::from_millis(60)).await;
            Ok(vec![Row { id: 1 }])
        });

        let stream = QueryStream::with_config(
            fetch,
            StreamingQueryConfig::default()
                .with_batch_size(10)
                .with_keepalive_interval(Duration::from_millis(10)),
        );

        let rows = collect_rows(stream.into_rows()).await;
        assert_eq!(rows, vec![Row { id: 1 }]);
    }

    #[test]
    fn keepalive_payloads_are_ignorable_by_their_transports() {
        // A blank line is skipped by every NDJSON reader.
        assert_eq!(NDJSON_KEEPALIVE, b"\n");
        // An SSE comment frame, which the EventSource spec requires clients to ignore.
        assert!(SSE_KEEPALIVE.starts_with(b":"));
        assert!(SSE_KEEPALIVE.ends_with(b"\n\n"));
    }

    // ── Cancellation ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn cancelling_stops_the_stream_without_another_batch() {
        let calls = Arc::new(AtomicUsize::new(0));
        let stream = QueryStream::with_config(
            table_fetcher(10_000, Arc::clone(&calls)),
            StreamingQueryConfig::default().with_batch_size(10),
        );
        let handle = stream.handle();
        handle.cancel();

        let rows = collect_rows(stream.into_rows()).await;

        assert!(rows.is_empty());
        assert_eq!(calls.load(Ordering::Relaxed), 0, "no batch should be fetched");
        assert!(handle.stats().cancelled);
    }

    #[tokio::test]
    async fn cancelling_mid_stream_stops_after_the_buffered_rows() {
        let calls = Arc::new(AtomicUsize::new(0));
        let stream = QueryStream::with_config(
            table_fetcher(10_000, Arc::clone(&calls)),
            StreamingQueryConfig::default().with_batch_size(10),
        );
        let handle = stream.handle();
        let mut rows = stream.into_rows();

        // Drain the first batch, then cancel before another is requested.
        let mut seen = 0;
        while let Some(Ok(_)) = rows.next().await {
            seen += 1;
            if seen == 10 {
                handle.cancel();
            }
            if seen > 200 {
                break;
            }
        }

        assert_eq!(seen, 10);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(handle.stats().cancelled);
    }

    #[test]
    fn a_handle_reports_its_cancellation_state() {
        let stream = QueryStream::new(table_fetcher(1, Arc::new(AtomicUsize::new(0))));
        let handle = stream.handle();

        assert!(!handle.is_cancelled());
        handle.cancel();
        assert!(handle.is_cancelled());
    }

    // ── Progress ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn progress_tracks_batches_rows_and_completion() {
        let calls = Arc::new(AtomicUsize::new(0));
        let stream = QueryStream::with_config(
            table_fetcher(30, Arc::clone(&calls)),
            StreamingQueryConfig::default().with_batch_size(10),
        );
        let handle = stream.handle();

        assert_eq!(handle.stats(), QueryStreamStats::default());

        let _ = collect_rows(stream.into_rows()).await;

        let stats = handle.stats();
        assert_eq!(stats.rows_emitted, 30);
        assert_eq!(stats.batches_fetched, 4);
        assert!(stats.completed);
        assert!(stats.exhausted);
        assert!(!stats.is_partial());
    }

    #[test]
    fn avg_batch_size_is_zero_before_the_first_batch() {
        assert!((QueryStreamStats::default().avg_batch_size() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn avg_batch_size_is_rows_over_batches() {
        let stats = QueryStreamStats {
            batches_fetched: 4,
            rows_emitted: 30,
            ..QueryStreamStats::default()
        };
        assert!((stats.avg_batch_size() - 7.5).abs() < f64::EPSILON);
    }

    #[test]
    fn a_stream_that_ran_out_of_rows_is_not_partial() {
        let stats = QueryStreamStats {
            completed: true,
            exhausted: true,
            ..QueryStreamStats::default()
        };
        assert!(!stats.is_partial());
    }
}
