//! Compression benchmark for GET /v1/events responses.
//!
//! Measures response time and compression ratio with and without gzip
//! compression at different response sizes (10, 100, 1000 events).
//!
//! # Running
//!
//! ```bash
//! cargo bench --bench compression
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::Write;

/// Generate a synthetic JSON response for N events.
fn generate_events_json(count: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"{\"data\":[");
    for i in 0..count {
        if i > 0 {
            buf.push(b',');
        }
        let event = format!(
            r#"{{"id":"00000000-0000-0000-0000-{:012}","contract_id":"CABC{}","event_type":"contract","tx_hash":"abc{}","ledger":{},"timestamp":"2026-03-14T00:00:00Z","event_data":{{"value":{{}},"topic":[]}},"created_at":"2026-03-14T00:00:01Z"}}"#,
            i, i % 100, i, 1000000 + i
        );
        buf.extend_from_slice(event.as_bytes());
    }
    buf.extend_from_slice(b"],\"total\":10000,\"page\":1,\"limit\":20,\"approximate\":true}");
    buf
}

/// Compress the given bytes with gzip at the default level (6).
fn compress_gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn bench_compression_ratio(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression/ratio");
    for size in [10, 100, 1000] {
        let json = generate_events_json(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &json, |b, json| {
            b.iter(|| {
                let compressed = compress_gzip(black_box(json));
                let ratio = json.len() as f64 / compressed.len() as f64;
                black_box(ratio)
            });
        });
    }
    group.finish();
}

fn bench_compression_time(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression/time");
    for size in [10, 100, 1000] {
        let json = generate_events_json(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &json, |b, json| {
            b.iter(|| {
                let compressed = compress_gzip(black_box(json));
                black_box(compressed.len())
            });
        });
    }
    group.finish();
}

fn bench_no_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression/none");
    for size in [10, 100, 1000] {
        let json = generate_events_json(size);
        group.bench_with_input(BenchmarkId::from_parameter(size), &json, |b, json| {
            b.iter(|| {
                black_box(json.len())
            });
        });
    }
    group.finish();
}

criterion_group!(
    compression_benches,
    bench_compression_ratio,
    bench_compression_time,
    bench_no_compression,
);
criterion_main!(compression_benches);
