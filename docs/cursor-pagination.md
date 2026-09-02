# Cursor (Keyset) Pagination

_Issue #962_

`GET /v1/events` supports two pagination strategies:

- **Offset pagination** (`page` + `limit`) — simple, but `OFFSET 100000` still
  makes Postgres scan and discard the first 100,000 matching rows, so cost
  grows with page depth.
- **Cursor (keyset) pagination** (`cursor` + `limit`) — encodes the sort
  column's value and row id from the last row of the previous page, and
  turns the query into `WHERE (sort_col, id) < ($1, $2) ORDER BY sort_col,
  id LIMIT n`. Cost is flat regardless of how deep into the result set the
  client is.

Cursor pagination is the recommended approach for consumers that page
through large result sets (indexers, exporters, the `spulse export` CLI
command) rather than jumping to an arbitrary page number.

## Using it

1. Make an initial request without `cursor`:

   ```
   GET /v1/events?limit=100&sort_by=ledger&sort=desc
   ```

2. The response includes `next_cursor` (an opaque, URL-safe base64 string)
   whenever more rows are available:

   ```json
   { "data": [...], "next_cursor": "bGVkZ2VyOjEyMzQ1NjppZC1oZXJl", "limit": 100 }
   ```

3. Pass it back to fetch the next page:

   ```
   GET /v1/events?limit=100&cursor=bGVkZ2VyOjEyMzQ1NjppZC1oZXJl
   ```

   `next_cursor` is `null`/absent once there are no more rows.

`sort_by` must stay the same across a paging session — the cursor is
tagged with the column it was generated from (`ledger`, `timestamp`, or
`created_at`), and a request with a mismatched `sort_by` is rejected:

```
400 Bad Request — "cursor sort column does not match sort_by"
```

All the same filters as offset mode (`contract_id`, `event_type`,
`from_ledger`/`to_ledger`, `topic_*`, etc.) can be combined with `cursor`.

## Cursor format

A cursor is `base64url(no padding)` of `"{tag}:{sort_value}:{row_id}"`,
e.g. `ledger:1234567:018f2e6a-....` before encoding. It is opaque to
clients by convention (don't construct or parse it — treat it as an
identifier), but the format is intentionally simple so it stays small.

Decoding rejects, with `400 Validation`:

- malformed base64 or non-UTF8 content
- a value that doesn't split into exactly `tag:value:id`
- a ledger cursor value that is `<= 0` or above `u32::MAX` (Stellar ledger
  sequences are 32-bit)
- an `id` that isn't a valid UUIDv4 (rules out both garbage and
  cursors forged from non-event-table ids)

See `encode_cursor_tagged` / `decode_cursor_tagged` in
[`src/handlers.rs`](../src/handlers.rs) for the implementation, and
`decode_cursor_rejects_*` in the same file's test module for the exact
rejection cases covered.

## Benchmarks

`benches/pagination.rs` includes `bench_cursor_encode` / `bench_cursor_decode`
alongside the existing offset-pagination benchmarks:

```bash
cargo bench --bench pagination
```

Keyset pagination's real win isn't encode/decode cost (both are
microseconds) — it's avoiding the `OFFSET n` scan in Postgres. That part
is measured against a live database in the `cursor_pagination_traverses_all_pages`
integration test, not the benchmark suite.
