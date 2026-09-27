## Summary

First slice of the batch-2 SDK items #1129-#1132: the Go SDK compiles again and its SSE stream is parsed correctly and no longer dies after the client timeout; the Go test suite runs and covers `GetEvents` over `httptest`; the Python async client runs on `httpx` (a declared dependency) instead of the undeclared `aiohttp`; and Go gets `VerifyWebhook` plus shared webhook test vectors. Each issue is only partly implemented; the lists below give the exact scope.

## Related Issue

Closes #1129, #1130, #1131, #1132 (see the trailing block).

## Changes

### #1129 Go SDK: SSE streaming support

What existed: `StreamEvents(ctx, contractID, handler)` in `client.go`, but the package did not compile on `main` (`io.NewReader` does not exist, unused `buffer`, unused `bytes`/`strconv` imports). Even with that fixed, the stream only handled single-line `data: ` frames and was killed after `ClientConfig.Timeout` (30 s by default), because `http.Client.Timeout` also bounds reading the body.

Done:
- `sse.go`: WHATWG-style event-stream parser (multi-line `data`, `:` keep-alive comments, `event`, `id` tracking via `LastEventID`, CRLF, `retry` ignored, unterminated frames dropped)
- `StreamEvents` uses it; non-event frames are skipped; returns `ctx.Err()` on cancellation, `nil` when the server closes the stream
- Streams go through the existing retry loop but on a copy of the HTTP client with no overall `Timeout`, so their lifetime is governed by `ctx`
- `sse_test.go` with `httptest.Server`: parser cases, delivery + headers, a stream that outlives a 50 ms client timeout (fails on the old client), handler error stops the stream, ctx cancel, HTTP 400

Not done in this PR:
- Channel-based `StreamEvents(ctx, StreamOptions) (<-chan Event, <-chan error)` API
- `Last-Event-ID` resume and reconnect with backoff (the parser already tracks the last id)
- README example

### #1130 Go SDK: unit tests for the client and models

What existed: `retry_policy_test.go` only, and it could not run: no `go.sum`, an unused `resp` variable, and the package itself did not compile (#1129). `GetEvents` and the other JSON methods never checked the status code or closed the body, so a 4xx/5xx decoded into an empty "successful" response.

Done:
- `go.sum` added, `go.mod` tidied (drops the unused `stretchr/objx`), the unused `resp` now backs a `ShouldRetry` assertion
- `APIError{StatusCode, Body}` + `decodeJSON` (closes the body, returns `*APIError` on non-2xx) used by `GetEvents`, `GetEventsByContract`, `GetEventsByTransactionHash`, `GetHealth`
- Table-driven `GetEvents` test over `httptest.Server`: success (query string, API key, decoding), 4xx not retried, 5xx retried then reported, timeout

Not done in this PR:
- Tests for the other client methods and for JSON fixtures from `openapi.json` examples
- CI job with `go vet`, `staticcheck`, `go test -race` and coverage in the job summary
- 80% coverage target (package is at 75.7%)

### #1131 Python SDK: async client and SSE streaming

What existed: `soroban_pulse.AsyncSorobanPulseClient` with `list_events`, `iter_events`, `create_subscription`, `wait_until_ready`, built on `aiohttp`, which is not a declared dependency (`docs/python-sdk.md` pointed to a `[async]` extra that does not exist). Network errors escaped as raw `aiohttp` exceptions, so `wait_until_ready` crashed instead of returning `False` on an unreachable server. `py.typed` already existed.

Done:
- `AsyncSorobanPulseClient` runs on `httpx.AsyncClient` (already a core dependency) with the sync client's error semantics: 401/403 -> `AuthenticationError`, other non-2xx -> `ApiError` (non-JSON bodies kept as `{"message": ...}`), network errors -> `ApiError(0, "network error: ...")`
- Optional `transport=` argument (e.g. `httpx.MockTransport`) for tests
- `tests/test_async_client.py` (`pytest-asyncio`, added to `test-requirements.txt`): params and auth header, cursor pagination, POST body, 401/403, 500 with text body, network error + `wait_until_ready`, use outside the context manager
- `docs/python-sdk.md` install and API table updated

Not done in this PR:
- `async for event in client.stream_events(contract_id=...)` with SSE parsing and resume
- Retry semantics matching the generated sync client
- Discord bot example

### #1132 Webhook signature verification helpers for Python and Go

What existed: the server signs deliveries with `X-Signature-256: sha256=<lowercase hex HMAC-SHA256(secret, raw body)>` (`src/webhook.rs` `sign_payload`, `docs/webhook_signing.md`). TypeScript and `openapi_client.webhook_verification` implement that. **`soroban_pulse.webhooks.verify_webhook_signature` implements a different scheme** (`t=<ts>,v1=<hmac of "ts.body">` in `X-SorobanPulse-Signature`) that the server never sends, so it rejects every real delivery. Not changed here.

Done:
- `sdk/testdata/webhook_vectors.json`: 10 shared vectors generated independently with Python's `hmac` (valid, rotation with a secondary secret, empty and unicode bodies, tampered body, wrong secret, truncated digest, wrong algorithm prefix, non-hex digest, missing header) with the expected failure class
- Go `VerifyWebhook(payload, signatureHeader, secrets...)`: constant-time `hmac.Equal`, checks every secret (rotation), sentinel errors `ErrMissingSignature` / `ErrInvalidSignatureFormat` / `ErrSignatureMismatch`, tested against all vectors
- Go README "Verifying Webhooks" section with a `net/http` handler

Not done in this PR:
- Python `verify_webhook(payload, headers, secret, tolerance=300)` and fixing `soroban_pulse.webhooks` to the real scheme
- Python and TypeScript tests consuming the shared vectors
- Timestamp tolerance (the live delivery path sends no timestamp header)
- Flask / FastAPI examples

## Testing

- [ ] `cargo test` passes (N/A: no Rust changes)
- [ ] `cargo clippy` reports no warnings (N/A: no Rust changes)
- [x] Manually tested locally

Verification:
- `sdk/go` (Go 1.22): `go vet ./...` clean, `go test -race -cover ./...` passes, 75.7% coverage (did not compile before). New files are `gofmt`-clean; existing files were not reformatted
- `sdk/python`: `pytest tests test` 36 passed (28 before + 8 new); `mypy` reports no errors in `async_client.py`

## Notes

The #1129 commit on its own needs #1130's `go.sum` to run the tests; the branch head is green.

Closes #1129
Closes #1130
Closes #1131
Closes #1132
