use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use soroban_pulse::models::PaginationParams;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Issue #962: cursor (keyset) pagination benchmarks
//
// `encode_cursor_tagged` / `decode_cursor_tagged` in `src/handlers.rs` are
// private to that module (they're an implementation detail of the
// `/v1/events` handler, not public API), so — following the same pattern
// `benches/compression.rs` uses for gzip — this mirrors their format
// ("{tag}:{value}:{id}", base64url no-pad) rather than reaching into the
// binary crate. Any change to the real encoding should be reflected here.
// ---------------------------------------------------------------------------

fn encode_cursor_tagged(tag: &str, value: &str, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(format!("{tag}:{value}:{id}"))
}

fn decode_cursor_tagged(cursor: &str) -> Option<(String, String, Uuid)> {
    let bytes = URL_SAFE_NO_PAD.decode(cursor).ok()?;
    let s = std::str::from_utf8(&bytes).ok()?;
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() != 3 {
        return None;
    }
    let id = Uuid::parse_str(parts[2]).ok()?;
    Some((parts[0].to_string(), parts[1].to_string(), id))
}

fn make(page: Option<i64>, limit: Option<i64>) -> PaginationParams {
    PaginationParams {
        page,
        limit,
        exact_count: None,
        fields: None,
        event_type: None,
        from_ledger: None,
        to_ledger: None,
        cursor: None,
        sort: None,
    }
}

fn bench_offset(c: &mut Criterion) {
    c.bench_function("PaginationParams::offset page=5 limit=20", |b| {
        let p = make(black_box(Some(5)), black_box(Some(20)));
        b.iter(|| p.offset());
    });
}

fn bench_limit(c: &mut Criterion) {
    c.bench_function("PaginationParams::limit clamp=200", |b| {
        let p = make(black_box(None), black_box(Some(200)));
        b.iter(|| p.limit());
    });
}

fn bench_cursor_encode(c: &mut Criterion) {
    let id = Uuid::new_v4();
    c.bench_function("cursor::encode ledger tag", |b| {
        b.iter(|| encode_cursor_tagged(black_box("ledger"), black_box("1234567"), black_box(id)));
    });
}

fn bench_cursor_decode(c: &mut Criterion) {
    let id = Uuid::new_v4();
    let cursor = encode_cursor_tagged("ledger", "1234567", id);
    c.bench_function("cursor::decode ledger tag", |b| {
        b.iter(|| decode_cursor_tagged(black_box(&cursor)));
    });
}

criterion_group!(
    benches,
    bench_offset,
    bench_limit,
    bench_cursor_encode,
    bench_cursor_decode
);
criterion_main!(benches);
