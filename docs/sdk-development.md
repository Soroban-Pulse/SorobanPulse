# SDK Development Guide

This guide is for contributors building, maintaining, or extending the Soroban Pulse client SDKs — architecture and design decisions, a tutorial per supported language, webhook signature verification per SDK, and error-handling best practices.

Looking to *use* an SDK in an application rather than develop one? See [docs/sdk-integration-guide.md](sdk-integration-guide.md) for the consumer-facing quickstart, subscription patterns, and streaming reference. This guide covers the same SDKs from the maintainer's side: why they're structured the way they are, and how to extend them consistently.

## Table of Contents

- [SDK Architecture and Design](#sdk-architecture-and-design)
- [TypeScript / JavaScript SDK Tutorial](#typescript--javascript-sdk-tutorial)
- [Python SDK Tutorial](#python-sdk-tutorial)
- [Go SDK Tutorial](#go-sdk-tutorial)
- [Rust Integration (No Dedicated SDK Yet)](#rust-integration-no-dedicated-sdk-yet)
- [Webhook Verification Per SDK](#webhook-verification-per-sdk)
- [Error Handling Best Practices](#error-handling-best-practices)

---

## SDK Architecture and Design

### Generation strategy

The TypeScript and Python SDKs are **generated** from [openapi.json](../openapi.json) plus a small amount of hand-written glue for behavior the spec can't express (SSE streaming, retry policies, webhook verification). The Go SDK is currently **hand-written** against the same spec, mirroring the generated clients' shape by convention rather than machinery.

```bash
make gen-openapi     # regenerate openapi.json from handler signatures
make generate-sdk    # regenerate the TypeScript and Python SDKs from openapi.json
```

Run `make gen-openapi` first whenever a handler's request/response shape changes — `generate-sdk` reads the checked-in spec, not the live server, so a stale spec produces a stale SDK. After regenerating, the hand-written files (`sse.ts`, `webhook-verification.ts`, `retry-policy.ts` and their Python equivalents) are **not** touched by the generator; only `apis/`, `models/`, and the Python `openapi_client/` package contents are replaced.

### Why a hybrid approach

Pure codegen covers CRUD-shaped REST calls well but has no vocabulary for:
- **Streaming** (SSE reconnection, `Last-Event-ID` tracking) — hand-written per language
- **Retry/backoff policy** — hand-written so behavior is consistent and independently testable (see `retry_policy_test.go`, and the Python/TS `RETRY_AND_BACKOFF.md` files)
- **Webhook signature verification** — security-sensitive; kept as reviewed, hand-written code rather than generated

This split means: touch the generator output for anything that's a plain API shape change, and touch the hand-written modules directly for anything behavioral.

### Consistent module layout

Every SDK follows the same rough shape so a contributor moving between languages can find things in the same relative place:

| Concern | TypeScript | Python | Go |
|---|---|---|---|
| Generated API classes | `apis/` | `openapi_client/api/` | `client.go` (hand-written) |
| Generated models | `models/` | `openapi_client/models/` | `models.go` |
| HTTP transport + retry | `runtime.ts`, `retry-policy.ts` | `openapi_client/rest.py`, retry config | `client.go`, `retry_policy.go` |
| SSE streaming | `sse.ts` | inline (see [Streaming Events](sdk-integration-guide.md#streaming-events)) | `client.go: StreamEvents` |
| Webhook verification | `webhook-verification.ts` | `openapi_client/webhook_verification.py` | none yet — see [below](#webhook-verification-per-sdk) |
| Usage examples | `examples.ts` | `examples.py` | inline in `README.md` |

### Design principles for new SDK work

1. **Match the wire contract exactly.** Field names, enum values, and status code semantics come from `openapi.json` — don't invent client-side names that diverge from the spec, even if a different name reads better in that language's idiom (case conventions are the one exception: `snake_case` in Python, `camelCase` in TypeScript, matching each language's own convention rather than the JSON body's).
2. **Retry only idempotent operations by default**, and only on the status codes in the [HTTP error reference](sdk-integration-guide.md#http-error-reference) (`429`, `500`, `502`, `503`, `504`). Never retry on `4xx` client errors other than `429`.
3. **Every network-facing default should be overridable** (base URL, timeout, retry count, max delay) — hardcoding these forces a fork for anyone self-hosting Soroban Pulse.
4. **New behavioral modules (streaming, verification, retry) get their own file**, not bolted onto the generated API classes, so regeneration never risks clobbering them.

---

## TypeScript / JavaScript SDK Tutorial

Source: [sdk/typescript/](../sdk/typescript/).

1. **Install dependencies and build:**
   ```bash
   cd sdk/typescript
   npm install
   npm run build   # emits to dist/, if a build script is configured
   ```
2. **Run the existing tests** before changing anything, to get a known-good baseline:
   ```bash
   npm test   # runs __tests__/
   ```
3. **Adding a new generated endpoint**: don't hand-edit `apis/` or `models/` — add the route to the Rust handler, run `make gen-openapi && make generate-sdk`, then diff the regenerated files to confirm only the expected endpoint changed.
4. **Adding hand-written behavior** (e.g. a new retry preset): add it to `retry-policy.ts`, export it from `index.ts`, and add a matching entry to [`RETRY_AND_BACKOFF.md`](../sdk/typescript/RETRY_AND_BACKOFF.md) and a usage example in `examples.ts`.
5. **Manual smoke test against a local server:**
   ```bash
   # terminal 1
   make run
   # terminal 2
   cd sdk/typescript && node -e "
     const { EventsApi, Configuration } = require('./dist');
     const api = new EventsApi(new Configuration({ basePath: 'http://localhost:3000' }));
     api.getEvents({ page: 1, limit: 5 }).then(r => console.log(r.data));
   "
   ```

See [docs/sdk-integration-guide.md § TypeScript / JavaScript Guide](sdk-integration-guide.md#typescript--javascript-guide) for the full module reference and interceptor API once you're extending rather than just reading the code.

---

## Python SDK Tutorial

Source: [sdk/python/](../sdk/python/).

1. **Set up a virtualenv and install in editable mode:**
   ```bash
   cd sdk/python
   python -m venv .venv && source .venv/bin/activate
   pip install -e .
   pip install -r test-requirements.txt
   ```
2. **Run tests:**
   ```bash
   tox                 # runs the full matrix defined in tox.ini
   # or, faster inner loop:
   pytest test/
   ```
3. **Regenerating from the spec** follows the same `make generate-sdk` path as TypeScript — it overwrites `openapi_client/api/` and `openapi_client/models/` only. `webhook_verification.py`, retry policy presets, and `examples.py` are hand-written and untouched by regeneration.
4. **Publishing a local build for testing in another project:**
   ```bash
   python setup.py sdist bdist_wheel
   pip install dist/soroban_pulse_client-*.whl --force-reinstall
   ```
5. **Docs**: the `sdk/python/docs/` directory holds per-model/per-API generated markdown — regenerate it alongside the client rather than editing by hand.

See [docs/sdk-integration-guide.md § Python Guide](sdk-integration-guide.md#python-guide) for async usage patterns, pagination helpers, and the retry policy presets (`create_default_retry_policy`, `create_aggressive_retry_policy`, `create_conservative_retry_policy`).

---

## Go SDK Tutorial

Source: [sdk/go/](../sdk/go/). This client is hand-written (no generator in the loop), so changes are made directly against `client.go` and `models.go`.

1. **Build and test:**
   ```bash
   cd sdk/go
   go build ./...
   go test ./...        # includes retry_policy_test.go
   ```
2. **Adding a new endpoint method**: add the request/response types to `models.go` (matching the field names and JSON tags in `openapi.json` exactly — see [design principle #1](#design-principles-for-new-sdk-work)), then add the method to `client.go` following the existing pattern (accept `context.Context` first, return `(*T, error)`).
3. **Testing retry behavior in isolation** without hitting a real server — `retry_policy_test.go` uses an `httptest.Server` that returns configured status codes; follow that pattern for new retry-related tests rather than requiring a live Soroban Pulse instance.
4. **Local module replace for testing against another project:**
   ```bash
   # in the consuming project's go.mod
   replace github.com/soroban-pulse/client-go => /absolute/path/to/SorobanPulse/sdk/go
   ```
5. **Keeping the README example current**: `sdk/go/README.md` carries the canonical usage example since there's no generated `examples.go` — update it whenever `client.go`'s public API changes shape.

See [docs/sdk-integration-guide.md § Go Guide](sdk-integration-guide.md#go-guide) for context lifecycle, connection pooling guidance, and the full processing-loop example.

---

## Rust Integration (No Dedicated SDK Yet)

There is currently no published `soroban-pulse-client` Rust crate — Rust consumers (including this repo's own integration tests) talk to the API directly with `reqwest` against the types in `src/models.rs`, or against types generated from [openapi.json](../openapi.json).

### Direct integration with `reqwest`

```rust
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct PaginatedEvents {
    data: Vec<serde_json::Value>,
    total: u64,
}

async fn get_events(client: &Client, base_url: &str, api_key: &str) -> anyhow::Result<PaginatedEvents> {
    let resp = client
        .get(format!("{base_url}/v1/events"))
        .header("X-Api-Key", api_key)
        .query(&[("page", "1"), ("limit", "20")])
        .send()
        .await?
        .error_for_status()?
        .json::<PaginatedEvents>()
        .await?;
    Ok(resp)
}
```

For retry-on-`429`/`5xx` semantics matching the other SDKs (see [design principle #2](#design-principles-for-new-sdk-work)), pair `reqwest` with [`reqwest-retry`](https://crates.io/crates/reqwest-retry) and [`backoff`](https://crates.io/crates/backoff) rather than hand-rolling a retry loop.

### Generating a typed client instead

If you need full generated model types rather than hand-written structs, run [`progenitor`](https://github.com/oxidecomputer/progenitor) or `openapi-generator generate -g rust -i openapi.json` against the checked-in spec. Neither is wired into `make generate-sdk` yet — if you build this out as a maintained crate under `sdk/rust/`, follow the [module layout](#consistent-module-layout) used by the other SDKs (separate files for streaming and webhook verification, not generated-code edits) and update the table above and `make generate-sdk` to include it.

### SSE streaming from Rust

```rust
use futures_util::StreamExt;
use eventsource_client::Client as SseClient;

let client = SseClient::for_url("http://localhost:3000/v1/events/stream")?
    .header("X-Api-Key", api_key)?
    .build();

let mut stream = client.stream();
while let Some(event) = stream.next().await {
    match event {
        Ok(eventsource_client::SSE::Event(ev)) => {
            let data: serde_json::Value = serde_json::from_str(&ev.data)?;
            process(data);
        }
        Ok(_) => {}
        Err(e) => eprintln!("stream error: {e}"),
    }
}
```

---

## Webhook Verification Per SDK

Every webhook request carries an `X-Signature-256: sha256=<hex_digest>` header. The signature algorithm itself (HMAC-SHA256 over the raw request body, constant-time comparison) is specified once in [docs/webhook_signing.md](webhook_signing.md) — the per-language snippets below call the SDK's built-in helper where one exists, and show the equivalent inline for SDKs that don't have one yet.

### TypeScript (built-in helper)

```typescript
import { verifyWebhookSignature } from "./sdk/typescript/webhook-verification";

app.post("/webhook", (req, res) => {
  const result = verifyWebhookSignature(
    req.rawBody,                                    // must be the raw, unparsed body
    req.headers["x-signature-256"] as string,
    process.env.WEBHOOK_SECRET!,
  );
  if (!result.isValid) return res.status(401).send(result.error);
  handle(req.body);
  res.sendStatus(200);
});
```

### Python (built-in helper)

```python
from openapi_client.webhook_verification import verify_webhook_signature

@app.post("/webhook")
async def webhook(request: Request):
    body = await request.body()
    is_valid, error = verify_webhook_signature(
        body, request.headers["X-Signature-256"], os.environ["WEBHOOK_SECRET"]
    )
    if not is_valid:
        raise HTTPException(status_code=401, detail=error)
    handle(await request.json())
    return {"status": "ok"}
```

### Go (no helper yet — inline using stdlib)

```go
import (
    "crypto/hmac"
    "crypto/sha256"
    "encoding/hex"
    "net/http"
    "strings"
)

func verifyWebhookSignature(body []byte, header, secret string) bool {
    sig, ok := strings.CutPrefix(header, "sha256=")
    if !ok {
        return false
    }
    mac := hmac.New(sha256.New, []byte(secret))
    mac.Write(body)
    expected := hex.EncodeToString(mac.Sum(nil))
    return hmac.Equal([]byte(sig), []byte(expected))
}

func webhookHandler(w http.ResponseWriter, r *http.Request) {
    body, _ := io.ReadAll(r.Body)
    if !verifyWebhookSignature(body, r.Header.Get("X-Signature-256"), os.Getenv("WEBHOOK_SECRET")) {
        http.Error(w, "invalid signature", http.StatusUnauthorized)
        return
    }
    // handle(body)
    w.WriteHeader(http.StatusOK)
}
```

This is a reasonable candidate to promote into a `VerifyWebhookSignature` function in `client.go` if it's needed more than once — see [design principle #4](#design-principles-for-new-sdk-work).

### Rust (no SDK — see [docs/webhook_signing.md](webhook_signing.md) for the full example)

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

fn verify_webhook_signature(header_value: &str, secret: &str, body: &[u8]) -> Result<(), &'static str> {
    let sig_hex = header_value.strip_prefix("sha256=").ok_or("invalid signature header format")?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| "invalid key length")?;
    mac.update(body);
    let expected = hex::encode(mac.finalize().into_bytes());
    if hex::decode(sig_hex).ok().as_deref() == Some(&hex::decode(&expected).unwrap()[..]) {
        Ok(())
    } else {
        Err("signature mismatch")
    }
}
```

Prefer `subtle`'s constant-time byte comparison (or decode both sides to bytes and use `subtle::ConstantTimeEq`) over `==` on the decoded bytes in real code — the snippet above decodes both sides specifically to avoid comparing the hex strings directly, but a dedicated constant-time comparison crate is safer than relying on that pattern being preserved during a future edit.

---

## Error Handling Best Practices

These apply across all SDKs, generated or hand-written:

1. **Distinguish retryable from terminal errors at the type level, not by re-inspecting the status code at every call site.** Every SDK here already does this (TypeScript throws after retries exhaust; Python raises `ApiException` with `.status`; Go returns a typed error) — extend that pattern rather than introducing raw status-code checks in application-facing code.
2. **Never swallow a `429` silently.** Surface `Retry-After` (or the SDK's parsed equivalent) to the caller, or handle it internally with backoff — don't retry immediately in a loop, which turns a rate limit into a self-inflicted denial of service against your own client. See [docs/subscription-best-practices.md § Client-side REST polling backoff](subscription-best-practices.md#client-side-rest-polling-backoff).
3. **Preserve the server's error body.** Soroban Pulse returns a structured JSON error body on 4xx/5xx responses; include it in the exception/error message rather than just the status code, since it usually names the exact invalid field.
4. **Fail closed on webhook verification.** Any error while parsing the signature header (missing prefix, wrong length, decode failure) must be treated as an invalid signature, not skipped — see the `Result`/exception-based designs above; none of them have a silent bypass path.
5. **Log with the request's correlation ID, not just the error message**, so a failure in an SDK can be cross-referenced against server-side logs — see [docs/logging.md](logging.md) for the `correlation_id` field convention. Every SDK's HTTP client should read the response's correlation/request-id header (where present) and attach it to thrown/returned errors.
6. **Time out every request explicitly.** Don't rely on a language's default (often "never") — each SDK exposes a configurable timeout (`Configuration.timeout`, `context.WithTimeout`, `aiohttp.ClientTimeout`); set one before shipping integration code, not just during debugging.
7. **Idempotency on retry.** Only the operations documented as safe to retry in the [HTTP error reference](sdk-integration-guide.md#http-error-reference) should be retried automatically. If you add a new mutating endpoint (e.g. a future `POST` that isn't naturally idempotent), exclude it from the default retry set explicitly rather than relying on callers to opt out.

---

## Related Documentation

- [SDK Integration Guide](sdk-integration-guide.md) — consumer-facing usage reference
- [Webhook Signing](webhook_signing.md) — the signature algorithm itself
- [Subscription Best Practices](subscription-best-practices.md)
- [OpenAPI Spec](../openapi.json)
- [Troubleshooting Guide](troubleshooting.md)
