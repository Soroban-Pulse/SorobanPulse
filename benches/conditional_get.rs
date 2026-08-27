use criterion::{criterion_group, criterion_main, Criterion};
use chrono::Utc;
use uuid::Uuid;
use soroban_pulse::conditional_get;

// Benchmark ETag computation performance
pub fn bench_etag_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("etag_computation");
    let id = Uuid::new_v4();
    let time = Utc::now();

    group.bench_function("compute_etag_simple", |b| {
        b.iter(|| {
            conditional_get::compute_etag_from_event(&id, &time)
        })
    });

    group.bench_function("compute_etag_with_count", |b| {
        b.iter(|| {
            conditional_get::compute_etag_from_event_with_count(&id, &time, Some(1000))
        })
    });

    group.finish();
}

// Benchmark conditional GET header validation
pub fn bench_conditional_get_logic(c: &mut Criterion) {
    use axum::http::HeaderMap;

    let mut group = c.benchmark_group("conditional_get_logic");
    let etag = "\"abc123def456\"";
    let time = Utc::now();

    // Benchmark 304 check with ETag match
    group.bench_function("etag_match_check", |b| {
        let mut headers = HeaderMap::new();
        headers.insert("if-none-match", etag.parse().unwrap());
        b.iter(|| {
            conditional_get::should_return_304(&headers, etag, &time)
        })
    });

    // Benchmark 304 check with ETag mismatch
    group.bench_function("etag_mismatch_check", |b| {
        let mut headers = HeaderMap::new();
        headers.insert("if-none-match", "\"different\"".parse().unwrap());
        b.iter(|| {
            conditional_get::should_return_304(&headers, etag, &time)
        })
    });

    // Benchmark without headers
    group.bench_function("no_conditional_headers", |b| {
        let headers = HeaderMap::new();
        b.iter(|| {
            conditional_get::should_return_304(&headers, etag, &time)
        })
    });

    group.finish();
}

// Benchmark ConditionalHeaders creation from events
pub fn bench_conditional_headers_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("conditional_headers");

    // Single event
    group.bench_function("from_single_event", |b| {
        let events = vec![(Uuid::new_v4(), Utc::now())];
        b.iter(|| {
            conditional_get::ConditionalHeaders::from_events(&events)
        })
    });

    // Multiple events
    group.bench_function("from_multiple_events", |b| {
        let events: Vec<_> = (0..100)
            .map(|i| (Uuid::new_v4(), Utc::now() - chrono::Duration::seconds(i as i64)))
            .collect();
        b.iter(|| {
            conditional_get::ConditionalHeaders::from_events(&events)
        })
    });

    // With count
    group.bench_function("from_events_with_count", |b| {
        let events = vec![(Uuid::new_v4(), Utc::now())];
        b.iter(|| {
            conditional_get::ConditionalHeaders::from_events_with_count(&events, Some(50000))
        })
    });

    group.finish();
}

// Benchmark bandwidth savings (simulated)
pub fn bench_bandwidth_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("bandwidth_comparison");

    // Simulate a typical 200 response with event data
    let response_body_200 = r#"{"events":[{"id":"123","data":{"key":"value"}},{"id":"124","data":{"key":"value"}}]}"#;
    let response_headers_200_bytes = 256; // typical response headers
    let total_200_bytes = response_body_200.len() + response_headers_200_bytes;

    // 304 response is just headers
    let response_headers_304_bytes = 128;

    group.bench_function("200_response_transmission", |b| {
        b.iter(|| {
            // Simulate 200 response size
            criterion::black_box(total_200_bytes)
        })
    });

    group.bench_function("304_response_transmission", |b| {
        b.iter(|| {
            // Simulate 304 response size
            criterion::black_box(response_headers_304_bytes)
        })
    });

    println!(
        "Bandwidth savings: {:.2}% when using 304",
        (1.0 - (response_headers_304_bytes as f64 / total_200_bytes as f64)) * 100.0
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_etag_computation,
    bench_conditional_get_logic,
    bench_conditional_headers_creation,
    bench_bandwidth_comparison
);
criterion_main!(benches);
