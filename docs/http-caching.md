# HTTP Caching

Soroban Pulse adds standard HTTP caching headers to cacheable read endpoints
so clients, CDNs, and shared caches can avoid unnecessary round trips.

Implementation: [`src/http_caching.rs`](../src/http_caching.rs), built on top
of the ETag/If-Modified-Since primitives in
[`src/conditional_get.rs`](../src/conditional_get.rs).

## Cache-Control

`CachePolicy` describes the caching behavior for a resource class:

```rust
use soroban_pulse::http_caching::CachePolicy;

let policy = CachePolicy::new("events.list", 60)
    .public()
    .with_stale_while_revalidate(30);
// => "public, max-age=60, must-revalidate, stale-while-revalidate=30"
```

Endpoints that must never be cached (auth, session data) use
`CachePolicy::no_store(...)`, which renders `Cache-Control: no-store`.

## ETag Generation

ETags are content-derived (last event id + timestamp, optionally + total
count) via `conditional_get::compute_etag_from_event[_with_count]`, so any
change to the underlying resource produces a different ETag.

## Last-Modified

`format_http_date` renders timestamps as RFC 7231 HTTP-dates
(`Thu, 27 Aug 2026 12:00:00 GMT`) for the `Last-Modified` header.

## Conditional Request Handling & Revalidation

`revalidate(resource, headers, etag, last_modified)` evaluates
`If-None-Match` and `If-Modified-Since` against current resource state:

```rust
match revalidate("events.detail", &req_headers, &etag, &last_modified) {
    RevalidationOutcome::NotModified => // return 304, no body
    RevalidationOutcome::Fresh => // return 200 with build_cache_headers(...)
}
```

## Cache Effectiveness Metrics

Every revalidation call records `soroban_pulse_http_cache_results_total{resource,result}`
(`hit` = 304 served, `miss` = full response served), letting you compute a
per-resource cache hit rate (`CacheEffectivenessSnapshot::hit_rate`).
