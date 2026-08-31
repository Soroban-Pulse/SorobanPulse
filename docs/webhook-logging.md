# Webhook Request/Response Logging

Every webhook delivery attempt (success or failure) is recorded to the
`webhook_logs` table via `src/webhook_logging.rs`, called from
`webhook::deliver_with_retry_policy` in `src/webhook.rs`. Logging is
best-effort and runs on a spawned task so it never adds latency to
delivery or retry timing.

## What's stored

Per attempt: destination URL, request headers, request body, response
status, response body, duration in milliseconds, and the originating
`contract_id`/`event_type`, in the `webhook_logs` table
(`migrations/20260831000001_webhook_request_response_logging.sql`).

## Size limits

Request/response bodies larger than `webhook_logging::MAX_LOGGED_BODY_BYTES`
(16 KiB) are not stored verbatim. Instead the stored value is replaced with:

```json
{"_truncated": true, "original_size_bytes": 123456, "preview": "first 512 chars..."}
```

and the corresponding `request_truncated`/`response_truncated` boolean
column is set, so callers can distinguish "empty body" from "body too
large to store."

## Sensitive data masking

Before storage, `webhook_logging::mask_sensitive` recursively masks any
JSON object field whose name contains one of a known set of
secret/credential terms (`secret`, `password`, `token`, `api_key`,
`authorization`, `private_key`, `access_token`, `refresh_token`,
`client_secret`, etc.), replacing the value with `"***"`.
`webhook_logging::mask_headers` applies the same masking to the flat
header list (so `Authorization` and `X-Signature-256` are never persisted
in the clear).

## Retention policy

`webhook_logging::purge_expired(pool, retention_days)` deletes rows older
than `retention_days` (default `DEFAULT_RETENTION_DAYS = 30`). Run it
periodically alongside the project's other archival jobs (see
`src/archiver.rs`) — it is not scheduled automatically by this change.

## Access controls and audit

Reading logs requires a role recognized by
`webhook_logging::is_authorized_to_read_logs` (`admin`, `operator`, or
`support`). Every call to `search` or `export_ndjson` — success or
denial — is expected to originate from an authenticated caller; every
successful search/export additionally writes a row to
`webhook_log_access_audit` recording the accessor, action, a summary of
the filter used, and the result count, so log access is itself
auditable.

## Search, filtering, and export

`webhook_logging::WebhookLogFilter` supports filtering by `url`,
`contract_id`, `response_status`, and a `since`/`until` time range, with
`limit`/`offset` pagination (default limit 100). `search()` returns
matching rows; `export_ndjson()` runs the same search and serializes the
results as newline-delimited JSON for download/archival.

## Testing

`src/webhook_logging.rs` includes unit tests covering: masking of
top-level and nested sensitive fields, header masking, body truncation
behavior above/below the size limit, and the role-based access control
allow-list.
