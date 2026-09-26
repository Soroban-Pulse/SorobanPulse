//! Streaming JSON responses for large result sets (Issue #688, optimized in #958).
//!
//! The point of this module is that a large response never exists in memory as a
//! whole. Rows are serialized one at a time, batched into chunks, and handed to
//! the HTTP layer as they are produced, so peak memory is bounded by
//! `StreamingOptions::buffer_size` rather than by the size of the result set.
//!
//! Four things make that bound real rather than nominal:
//!
//! * **Chunking** — items are coalesced into `buffer_size` byte chunks instead of
//!   one channel send per row. A million-row response costs a million small
//!   serializations either way, but not a million channel sends and wakeups.
//! * **Backpressure** — the producer task and the HTTP writer are joined by a
//!   bounded channel. A slow client fills the channel, the producer parks on it,
//!   and the database stream stops being polled. Without the bound, a slow client
//!   would let the whole result set accumulate in the channel, which is exactly
//!   the memory blow-up streaming was meant to avoid.
//! * **Compression** — applied to each chunk as it goes past, with a sync flush
//!   per chunk, so the response stays streaming rather than being buffered to
//!   compress it in one go.
//! * **Cancellation** — a client that disconnects, or a caller that calls
//!   [`StreamHandle::cancel`], stops the producer at the next item instead of
//!   letting it drain a result set nobody is reading.

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use flate2::write::GzEncoder;
use futures::stream::{Stream, StreamExt};
use serde::Serialize;
use std::io::Write;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::error::TrySendError;
use tracing::{debug, instrument, warn};

pub type StreamingResultIterator<T> =
    Pin<Box<dyn Stream<Item = Result<T, sqlx::Error>> + Send + 'static>>;

/// Default bytes accumulated before a chunk is flushed to the client.
pub const DEFAULT_BUFFER_SIZE: usize = 8192;

/// Default number of chunks in flight between the producer and the HTTP writer.
///
/// Peak buffering is roughly `DEFAULT_BUFFER_SIZE * DEFAULT_CHANNEL_CAPACITY`.
/// Larger values smooth over jittery clients; smaller values apply backpressure
/// sooner. Four chunks is enough to keep the socket busy without letting a
/// stalled client hold much.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 4;

/// Content encoding applied to each chunk on its way out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamCompression {
    #[default]
    None,
    Gzip,
}

impl StreamCompression {
    /// The `Content-Encoding` header value, or `None` for an unencoded body.
    pub fn header_value(self) -> Option<&'static str> {
        match self {
            StreamCompression::None => None,
            StreamCompression::Gzip => Some("gzip"),
        }
    }

    /// Pick an encoding from a request's `Accept-Encoding` header.
    ///
    /// Anything we cannot encode falls back to identity, which is always a valid
    /// response, so an unusual header can never fail a request.
    pub fn negotiate(accept_encoding: Option<&str>) -> Self {
        match accept_encoding {
            Some(value) if value.to_ascii_lowercase().contains("gzip") => StreamCompression::Gzip,
            _ => StreamCompression::None,
        }
    }
}

/// Tunables for a single streaming response.
#[derive(Debug, Clone, Copy)]
pub struct StreamingOptions {
    pub buffer_size: usize,
    pub channel_capacity: usize,
    pub compression: StreamCompression,
}

impl Default for StreamingOptions {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_BUFFER_SIZE,
            channel_capacity: DEFAULT_CHANNEL_CAPACITY,
            compression: StreamCompression::None,
        }
    }
}

impl StreamingOptions {
    pub fn with_buffer_size(mut self, buffer_size: usize) -> Self {
        // A zero-byte buffer would flush per item and defeat chunking, so the
        // floor is one byte and the caller's intent (flush eagerly) is kept.
        self.buffer_size = buffer_size.max(1);
        self
    }

    pub fn with_channel_capacity(mut self, capacity: usize) -> Self {
        // tokio's bounded channel rejects a zero capacity outright.
        self.channel_capacity = capacity.max(1);
        self
    }

    pub fn with_compression(mut self, compression: StreamCompression) -> Self {
        self.compression = compression;
        self
    }
}

/// Live counters for an in-flight stream.
///
/// Shared between the producer task and whoever holds the [`StreamHandle`], so
/// progress is observable while the response is still being written rather than
/// only after it completes.
#[derive(Debug, Default)]
pub struct StreamProgress {
    items_sent: AtomicU64,
    chunks_sent: AtomicU64,
    bytes_before_encoding: AtomicU64,
    bytes_after_encoding: AtomicU64,
    serialization_errors: AtomicU64,
    database_errors: AtomicU64,
    backpressure_waits: AtomicU64,
    completed: AtomicBool,
    cancelled: AtomicBool,
}

impl StreamProgress {
    pub fn items_sent(&self) -> u64 {
        self.items_sent.load(Ordering::Relaxed)
    }

    pub fn is_complete(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }

    pub fn snapshot(&self) -> StreamingStats {
        let serialization_errors = self.serialization_errors.load(Ordering::Relaxed);
        let database_errors = self.database_errors.load(Ordering::Relaxed);

        StreamingStats {
            items_sent: self.items_sent.load(Ordering::Relaxed),
            chunks_sent: self.chunks_sent.load(Ordering::Relaxed),
            bytes_before_encoding: self.bytes_before_encoding.load(Ordering::Relaxed),
            bytes_after_encoding: self.bytes_after_encoding.load(Ordering::Relaxed),
            errors: serialization_errors + database_errors,
            serialization_errors,
            database_errors,
            backpressure_waits: self.backpressure_waits.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Acquire),
            cancelled: self.cancelled.load(Ordering::Acquire),
        }
    }
}

/// A point-in-time copy of a stream's counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreamingStats {
    pub items_sent: u64,
    pub chunks_sent: u64,
    pub bytes_before_encoding: u64,
    pub bytes_after_encoding: u64,
    pub errors: u64,
    pub serialization_errors: u64,
    pub database_errors: u64,
    pub backpressure_waits: u64,
    pub completed: bool,
    pub cancelled: bool,
}

impl StreamingStats {
    /// Encoded size as a fraction of the raw size. Returns 1.0 when nothing has
    /// been written yet, and for an uncompressed stream.
    pub fn compression_ratio(&self) -> f64 {
        if self.bytes_before_encoding == 0 {
            return 1.0;
        }
        self.bytes_after_encoding as f64 / self.bytes_before_encoding as f64
    }
}

/// Caller-side control of a stream: watch its progress, or stop it.
#[derive(Debug, Clone)]
pub struct StreamHandle {
    progress: Arc<StreamProgress>,
    cancel: Arc<AtomicBool>,
}

impl StreamHandle {
    fn new() -> Self {
        Self {
            progress: Arc::new(StreamProgress::default()),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Ask the producer to stop. It stops after the item it is on, so this is
    /// not instantaneous — but it does stop the database stream from being
    /// polled any further.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    pub fn progress(&self) -> &StreamProgress {
        &self.progress
    }

    pub fn stats(&self) -> StreamingStats {
        self.progress.snapshot()
    }
}

/// Per-chunk encoder. Gzip is flushed after every chunk so the client can
/// decode what it has received instead of waiting for the stream to end.
enum ChunkEncoder {
    Identity,
    Gzip(Box<GzEncoder<Vec<u8>>>),
}

impl ChunkEncoder {
    fn new(compression: StreamCompression) -> Self {
        match compression {
            StreamCompression::None => ChunkEncoder::Identity,
            StreamCompression::Gzip => ChunkEncoder::Gzip(Box::new(GzEncoder::new(
                Vec::new(),
                flate2::Compression::default(),
            ))),
        }
    }

    fn encode(&mut self, bytes: Vec<u8>) -> std::io::Result<Vec<u8>> {
        match self {
            ChunkEncoder::Identity => Ok(bytes),
            ChunkEncoder::Gzip(encoder) => {
                encoder.write_all(&bytes)?;
                // A sync flush emits a decodable deflate block boundary. Without
                // it the encoder would hold everything until finish() and the
                // response would not actually stream.
                encoder.flush()?;
                Ok(std::mem::take(encoder.get_mut()))
            }
        }
    }

    /// Emit the trailer (gzip CRC and length). Empty for identity.
    fn finish(self) -> std::io::Result<Vec<u8>> {
        match self {
            ChunkEncoder::Identity => Ok(Vec::new()),
            ChunkEncoder::Gzip(encoder) => (*encoder).finish(),
        }
    }
}

pub struct StreamingJsonResponse<T: Serialize + Send> {
    stream: StreamingResultIterator<T>,
    options: StreamingOptions,
    handle: StreamHandle,
}

impl<T: Serialize + Send + 'static> StreamingJsonResponse<T> {
    pub fn new(stream: StreamingResultIterator<T>) -> Self {
        Self::with_options(stream, StreamingOptions::default())
    }

    pub fn with_buffer_size(stream: StreamingResultIterator<T>, buffer_size: usize) -> Self {
        Self::with_options(
            stream,
            StreamingOptions::default().with_buffer_size(buffer_size),
        )
    }

    pub fn with_options(stream: StreamingResultIterator<T>, options: StreamingOptions) -> Self {
        Self {
            stream,
            options,
            handle: StreamHandle::new(),
        }
    }

    pub fn compressed(mut self, compression: StreamCompression) -> Self {
        self.options.compression = compression;
        self
    }

    pub fn buffer_size(&self) -> usize {
        self.options.buffer_size
    }

    pub fn options(&self) -> StreamingOptions {
        self.options
    }

    /// Handle for observing or cancelling this stream. Take it before the
    /// response is converted into a body — afterwards the response is consumed.
    pub fn handle(&self) -> StreamHandle {
        self.handle.clone()
    }

    /// Spawn the producer and return the body it feeds.
    ///
    /// Synchronous on purpose: it only spawns and wires up channels, so there is
    /// nothing to await, and blocking a runtime thread here (as an earlier
    /// version did via `block_on`) risked deadlocking the very executor meant to
    /// be draining the stream.
    #[instrument(skip(self))]
    fn into_streaming_body(self) -> Body {
        let StreamingJsonResponse {
            stream,
            options,
            handle,
        } = self;

        let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(options.channel_capacity);
        let progress = Arc::clone(&handle.progress);
        let cancel = Arc::clone(&handle.cancel);

        tokio::spawn(async move {
            let started = Instant::now();
            let mut stream = stream;
            let mut encoder = ChunkEncoder::new(options.compression);
            let mut buffer: Vec<u8> = Vec::with_capacity(options.buffer_size);
            let mut first = true;

            // Flush `buffer` through the encoder and into the channel, applying
            // backpressure when the consumer is behind. Returns false once the
            // receiver is gone, which is how a client disconnect gets noticed.
            macro_rules! flush {
                () => {{
                    if buffer.is_empty() {
                        true
                    } else {
                        let raw_len = buffer.len() as u64;
                        let taken = std::mem::replace(
                            &mut buffer,
                            Vec::with_capacity(options.buffer_size),
                        );
                        match encoder.encode(taken) {
                            Ok(encoded) => {
                                if encoded.is_empty() {
                                    progress
                                        .bytes_before_encoding
                                        .fetch_add(raw_len, Ordering::Relaxed);
                                    true
                                } else {
                                    let encoded_len = encoded.len() as u64;
                                    if send_chunk(&tx, encoded, &progress).await {
                                        progress
                                            .bytes_before_encoding
                                            .fetch_add(raw_len, Ordering::Relaxed);
                                        progress
                                            .bytes_after_encoding
                                            .fetch_add(encoded_len, Ordering::Relaxed);
                                        progress.chunks_sent.fetch_add(1, Ordering::Relaxed);
                                        crate::metrics::record_streaming_response_chunk(encoded_len);
                                        true
                                    } else {
                                        false
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "Compression error in streaming response");
                                crate::metrics::record_streaming_response_error("compression");
                                false
                            }
                        }
                    }
                }};
            }

            buffer.push(b'[');

            while let Some(result) = stream.next().await {
                if cancel.load(Ordering::Acquire) {
                    progress.cancelled.store(true, Ordering::Release);
                    debug!("Streaming response cancelled by caller");
                    crate::metrics::record_streaming_response_cancelled("caller");
                    break;
                }

                match result {
                    Ok(item) => match serde_json::to_vec(&item) {
                        Ok(json_bytes) => {
                            if !first {
                                buffer.push(b',');
                            }
                            buffer.extend_from_slice(&json_bytes);
                            first = false;
                            progress.items_sent.fetch_add(1, Ordering::Relaxed);
                            crate::metrics::record_streaming_response_item_sent();

                            if buffer.len() >= options.buffer_size && !flush!() {
                                progress.cancelled.store(true, Ordering::Release);
                                crate::metrics::record_streaming_response_cancelled("client");
                                return;
                            }
                        }
                        Err(e) => {
                            // One bad row must not abort a response that is
                            // already partly written — skip it and keep going.
                            debug!(error = %e, "Serialization error in streaming response");
                            progress
                                .serialization_errors
                                .fetch_add(1, Ordering::Relaxed);
                            crate::metrics::record_streaming_response_error("serialization");
                        }
                    },
                    Err(e) => {
                        debug!(error = %e, "Database error in streaming response");
                        progress.database_errors.fetch_add(1, Ordering::Relaxed);
                        crate::metrics::record_streaming_response_error("database");
                    }
                }
            }

            buffer.push(b']');
            let delivered = flush!();

            if delivered {
                match encoder.finish() {
                    Ok(trailer) if !trailer.is_empty() => {
                        let trailer_len = trailer.len() as u64;
                        if send_chunk(&tx, trailer, &progress).await {
                            progress
                                .bytes_after_encoding
                                .fetch_add(trailer_len, Ordering::Relaxed);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!(error = %e, "Failed to finish compressed stream");
                        crate::metrics::record_streaming_response_error("compression");
                    }
                }
            }

            let count = progress.items_sent.load(Ordering::Relaxed);
            progress.completed.store(true, Ordering::Release);

            debug!(
                items_sent = count,
                chunks_sent = progress.chunks_sent.load(Ordering::Relaxed),
                elapsed_ms = started.elapsed().as_millis() as u64,
                "Streaming response completed"
            );
            crate::metrics::record_streaming_response_completed(count);
            crate::metrics::record_streaming_response_duration(started.elapsed().as_secs_f64());
        });

        Body::from_stream(
            futures::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|bytes| (bytes, rx))
            })
            .map(Ok::<_, std::io::Error>),
        )
    }
}

/// Send one chunk, recording a backpressure event when the consumer is behind.
///
/// `try_send` first so the common case (consumer keeping up) costs nothing, then
/// the awaiting send, which is where the producer actually parks and stops
/// pulling rows out of the database.
async fn send_chunk(
    tx: &tokio::sync::mpsc::Sender<Vec<u8>>,
    chunk: Vec<u8>,
    progress: &StreamProgress,
) -> bool {
    match tx.try_send(chunk) {
        Ok(()) => true,
        Err(TrySendError::Closed(_)) => {
            debug!("Streaming response consumer went away");
            false
        }
        Err(TrySendError::Full(chunk)) => {
            progress.backpressure_waits.fetch_add(1, Ordering::Relaxed);
            crate::metrics::record_streaming_response_backpressure();
            tx.send(chunk).await.is_ok()
        }
    }
}

impl<T: Serialize + Send + 'static> IntoResponse for StreamingJsonResponse<T> {
    fn into_response(self) -> Response {
        let compression = self.options.compression;
        let body = self.into_streaming_body();

        let mut response = Response::new(body);
        let headers = response.headers_mut();
        headers.insert(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            axum::http::header::TRANSFER_ENCODING,
            axum::http::HeaderValue::from_static("chunked"),
        );
        if let Some(encoding) = compression.header_value() {
            headers.insert(
                axum::http::header::CONTENT_ENCODING,
                axum::http::HeaderValue::from_static(encoding),
            );
        }
        // The body length is unknown up front, so a cache or proxy must not
        // assume the response is complete from a Content-Length it invented.
        headers.insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store"),
        );

        response
    }
}

pub fn create_streaming_response<T: Serialize + Send + 'static>(
    stream: StreamingResultIterator<T>,
) -> StreamingJsonResponse<T> {
    StreamingJsonResponse::new(stream)
}

pub fn create_streaming_response_with_buffer<T: Serialize + Send + 'static>(
    stream: StreamingResultIterator<T>,
    buffer_size: usize,
) -> StreamingJsonResponse<T> {
    StreamingJsonResponse::with_buffer_size(stream, buffer_size)
}

/// Build a streaming response for any endpoint, negotiating compression from the
/// request's `Accept-Encoding`.
///
/// This is the entry point handlers should use: it keeps every endpoint's
/// streaming behaviour (chunk size, backpressure bound, encoding negotiation)
/// identical instead of each one picking its own.
pub fn create_negotiated_streaming_response<T: Serialize + Send + 'static>(
    stream: StreamingResultIterator<T>,
    accept_encoding: Option<&str>,
) -> StreamingJsonResponse<T> {
    StreamingJsonResponse::with_options(
        stream,
        StreamingOptions::default().with_compression(StreamCompression::negotiate(accept_encoding)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use flate2::read::GzDecoder;
    use futures::stream;
    use serde_json::json;
    use std::io::Read;

    const BODY_LIMIT: usize = 1024 * 1024;

    fn value_stream(count: usize) -> StreamingResultIterator<serde_json::Value> {
        Box::pin(stream::iter(
            (0..count)
                .map(|i| Ok::<serde_json::Value, sqlx::Error>(json!({ "id": i })))
                .collect::<Vec<_>>(),
        ))
    }

    async fn body_bytes(response: Response) -> Vec<u8> {
        to_bytes(response.into_body(), BODY_LIMIT)
            .await
            .expect("body should collect")
            .to_vec()
    }

    fn gunzip(bytes: &[u8]) -> String {
        let mut decoder = GzDecoder::new(bytes);
        let mut out = String::new();
        decoder.read_to_string(&mut out).expect("valid gzip stream");
        out
    }

    // ── Construction ─────────────────────────────────────────────────────────

    #[test]
    fn streaming_response_creation() {
        let response = StreamingJsonResponse::new(value_stream(2));
        assert_eq!(response.buffer_size(), DEFAULT_BUFFER_SIZE);
        assert_eq!(
            response.options().channel_capacity,
            DEFAULT_CHANNEL_CAPACITY
        );
        assert_eq!(response.options().compression, StreamCompression::None);
    }

    #[test]
    fn streaming_response_custom_buffer() {
        let response = StreamingJsonResponse::with_buffer_size(value_stream(1), 16384);
        assert_eq!(response.buffer_size(), 16384);
    }

    #[test]
    fn zero_sized_options_are_clamped_to_something_usable() {
        // A zero channel capacity panics inside tokio, and a zero buffer would
        // flush per item; both are clamped rather than trusted.
        let options = StreamingOptions::default()
            .with_buffer_size(0)
            .with_channel_capacity(0);
        assert_eq!(options.buffer_size, 1);
        assert_eq!(options.channel_capacity, 1);
    }

    // ── Encoding negotiation ─────────────────────────────────────────────────

    #[test]
    fn negotiates_gzip_only_when_offered() {
        assert_eq!(
            StreamCompression::negotiate(Some("gzip, deflate")),
            StreamCompression::Gzip
        );
        assert_eq!(
            StreamCompression::negotiate(Some("GZIP")),
            StreamCompression::Gzip
        );
        assert_eq!(
            StreamCompression::negotiate(Some("br")),
            StreamCompression::None
        );
        assert_eq!(StreamCompression::negotiate(None), StreamCompression::None);
    }

    #[test]
    fn identity_encoding_sets_no_content_encoding_header() {
        assert_eq!(StreamCompression::None.header_value(), None);
        assert_eq!(StreamCompression::Gzip.header_value(), Some("gzip"));
    }

    // ── Body content ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn emits_a_well_formed_json_array() {
        let response = StreamingJsonResponse::new(value_stream(3)).into_response();
        let body = body_bytes(response).await;

        let parsed: serde_json::Value =
            serde_json::from_slice(&body).expect("body should be valid JSON");
        assert_eq!(parsed, json!([{"id": 0}, {"id": 1}, {"id": 2}]));
    }

    #[tokio::test]
    async fn emits_an_empty_array_for_an_empty_stream() {
        let response = StreamingJsonResponse::new(value_stream(0)).into_response();
        let body = body_bytes(response).await;

        assert_eq!(String::from_utf8(body).unwrap(), "[]");
    }

    #[tokio::test]
    async fn small_buffer_still_produces_one_valid_document() {
        // Forces a flush per item, so the array separators have to be correct
        // across chunk boundaries rather than only within a chunk.
        let response = StreamingJsonResponse::with_options(
            value_stream(20),
            StreamingOptions::default().with_buffer_size(1),
        )
        .into_response();
        let body = body_bytes(response).await;

        let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON");
        assert_eq!(parsed.as_array().map(Vec::len), Some(20));
    }

    #[tokio::test]
    async fn database_errors_are_skipped_without_corrupting_the_array() {
        let items: Vec<Result<serde_json::Value, sqlx::Error>> = vec![
            Ok(json!({"id": 1})),
            Err(sqlx::Error::RowNotFound),
            Ok(json!({"id": 2})),
        ];
        let response = StreamingJsonResponse::new(Box::pin(stream::iter(items))).into_response();
        let body = body_bytes(response).await;

        let parsed: serde_json::Value = serde_json::from_slice(&body).expect("valid JSON");
        assert_eq!(parsed, json!([{"id": 1}, {"id": 2}]));
    }

    // ── Compression ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn gzip_body_decodes_to_the_same_document() {
        let response = StreamingJsonResponse::new(value_stream(5))
            .compressed(StreamCompression::Gzip)
            .into_response();

        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_ENCODING)
                .map(|v| v.to_str().unwrap().to_string()),
            Some("gzip".to_string())
        );

        let body = body_bytes(response).await;
        let text = gunzip(&body);
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed.as_array().map(Vec::len), Some(5));
    }

    #[tokio::test]
    async fn gzip_is_flushed_per_chunk_rather_than_only_at_the_end() {
        // With a one-byte buffer every item is its own chunk. If the encoder only
        // emitted at finish() the stream would not be incremental, and the chunk
        // count would collapse to one.
        let response = StreamingJsonResponse::with_options(
            value_stream(50),
            StreamingOptions::default()
                .with_buffer_size(1)
                .with_compression(StreamCompression::Gzip),
        );
        let handle = response.handle();
        let body = body_bytes(response.into_response()).await;

        let parsed: serde_json::Value =
            serde_json::from_str(&gunzip(&body)).expect("valid JSON");
        assert_eq!(parsed.as_array().map(Vec::len), Some(50));
        assert!(
            handle.stats().chunks_sent > 1,
            "expected multiple flushed chunks, got {}",
            handle.stats().chunks_sent
        );
    }

    // ── Headers ──────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn sets_chunked_transfer_encoding_and_no_store() {
        let response = StreamingJsonResponse::new(value_stream(1)).into_response();
        let headers = response.headers();

        assert_eq!(
            headers.get(axum::http::header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(
            headers.get(axum::http::header::TRANSFER_ENCODING).unwrap(),
            "chunked"
        );
        assert_eq!(
            headers.get(axum::http::header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
        assert!(headers
            .get(axum::http::header::CONTENT_ENCODING)
            .is_none());
    }

    // ── Progress tracking ────────────────────────────────────────────────────

    #[tokio::test]
    async fn progress_reports_items_chunks_and_bytes() {
        let response = StreamingJsonResponse::new(value_stream(10));
        let handle = response.handle();
        let _ = body_bytes(response.into_response()).await;

        let stats = handle.stats();
        assert_eq!(stats.items_sent, 10);
        assert!(stats.chunks_sent >= 1);
        assert!(stats.bytes_before_encoding > 0);
        assert_eq!(stats.bytes_after_encoding, stats.bytes_before_encoding);
        assert!(stats.completed);
        assert!(!stats.cancelled);
        assert_eq!(stats.errors, 0);
    }

    #[tokio::test]
    async fn progress_counts_errors_by_kind() {
        let items: Vec<Result<serde_json::Value, sqlx::Error>> =
            vec![Ok(json!({"id": 1})), Err(sqlx::Error::RowNotFound)];
        let response = StreamingJsonResponse::new(Box::pin(stream::iter(items)));
        let handle = response.handle();
        let _ = body_bytes(response.into_response()).await;

        let stats = handle.stats();
        assert_eq!(stats.database_errors, 1);
        assert_eq!(stats.serialization_errors, 0);
        assert_eq!(stats.errors, 1);
        assert_eq!(stats.items_sent, 1);
    }

    #[test]
    fn compression_ratio_is_one_before_anything_is_written() {
        assert!((StreamingStats::default().compression_ratio() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compression_ratio_reflects_encoded_size() {
        let stats = StreamingStats {
            bytes_before_encoding: 1000,
            bytes_after_encoding: 250,
            ..StreamingStats::default()
        };
        assert!((stats.compression_ratio() - 0.25).abs() < f64::EPSILON);
    }

    // ── Cancellation ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn cancelling_before_the_body_is_read_stops_the_producer_early() {
        let response = StreamingJsonResponse::with_options(
            value_stream(10_000),
            StreamingOptions::default().with_buffer_size(1),
        );
        let handle = response.handle();
        handle.cancel();

        let _ = body_bytes(response.into_response()).await;

        let stats = handle.stats();
        assert!(stats.cancelled);
        assert!(
            stats.items_sent < 10_000,
            "cancelled stream sent everything anyway: {}",
            stats.items_sent
        );
    }

    #[tokio::test]
    async fn a_handle_reports_cancellation_state() {
        let response = StreamingJsonResponse::new(value_stream(1));
        let handle = response.handle();

        assert!(!handle.is_cancelled());
        handle.cancel();
        assert!(handle.is_cancelled());
    }

    #[tokio::test]
    async fn dropping_the_body_stops_the_producer() {
        let response = StreamingJsonResponse::with_options(
            value_stream(100_000),
            StreamingOptions::default()
                .with_buffer_size(1)
                .with_channel_capacity(1),
        );
        let handle = response.handle();

        // Drop the body without reading it: the producer's sends start failing
        // and it must give up rather than serializing a hundred thousand rows.
        drop(response.into_response());

        for _ in 0..100 {
            if handle.stats().cancelled || handle.progress().is_complete() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        assert!(
            handle.stats().items_sent < 100_000,
            "producer kept going after the consumer went away"
        );
    }

    // ── Backpressure ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_slow_consumer_registers_backpressure() {
        let response = StreamingJsonResponse::with_options(
            value_stream(2_000),
            StreamingOptions::default()
                .with_buffer_size(1)
                .with_channel_capacity(1),
        );
        let handle = response.handle();
        let _ = body_bytes(response.into_response()).await;

        let stats = handle.stats();
        assert_eq!(stats.items_sent, 2_000);
        assert!(
            stats.backpressure_waits > 0,
            "producer never parked on a full channel"
        );
    }

    // ── Constructors ─────────────────────────────────────────────────────────

    #[test]
    fn negotiated_constructor_applies_the_client_encoding() {
        let response =
            create_negotiated_streaming_response(value_stream(1), Some("gzip, deflate"));
        assert_eq!(response.options().compression, StreamCompression::Gzip);

        let plain = create_negotiated_streaming_response(value_stream(1), None);
        assert_eq!(plain.options().compression, StreamCompression::None);
    }

    #[test]
    fn helper_constructors_match_the_builder() {
        assert_eq!(
            create_streaming_response(value_stream(1)).buffer_size(),
            DEFAULT_BUFFER_SIZE
        );
        assert_eq!(
            create_streaming_response_with_buffer(value_stream(1), 512).buffer_size(),
            512
        );
    }
}
