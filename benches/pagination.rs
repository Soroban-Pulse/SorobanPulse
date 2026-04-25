use criterion::{black_box, criterion_group, criterion_main, Criterion};
use soroban_pulse::models::PaginationParams;

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

criterion_group!(benches, bench_offset, bench_limit);
criterion_main!(benches);
