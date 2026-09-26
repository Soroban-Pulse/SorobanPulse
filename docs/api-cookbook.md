# API Cookbook

Worked examples for common tasks, each grounded in an endpoint documented in
`openapi.json`. For full parameter reference on any endpoint here, see
[`docs/api-usage.md`](api-usage.md); this doc focuses on end-to-end recipes.

All examples assume the service is running at `http://localhost:3000` and
that `$API_KEY` is set only if the server actually requires it (see
[`docs/api-guide.md`](api-guide.md#authentication)).

## Table of contents

1. [Subscribe to events for a contract with a webhook](#1-subscribe-to-events-for-a-contract-with-a-webhook)
2. [Paginate through historical events for a contract](#2-paginate-through-historical-events-for-a-contract)
3. [Verify a webhook signature](#3-verify-a-webhook-signature)
4. [Handle rate limiting with backoff](#4-handle-rate-limiting-with-backoff)
5. [Stream live events for one contract via SSE](#5-stream-live-events-for-one-contract-via-sse)
6. [Bulk-export events as NDJSON](#6-bulk-export-events-as-ndjson)
7. [Look up events for multiple transactions in one call](#7-look-up-events-for-multiple-transactions-in-one-call)
8. [Trace related events across a transaction chain](#8-trace-related-events-across-a-transaction-chain)

---

## 1. Subscribe to events for a contract with a webhook

`POST /subscriptions` registers a callback URL that receives new events from
a given ledger onward.

```bash
curl -X POST http://localhost:3000/subscriptions \
  -H "Content-Type: application/json" \
  -d '{
    "callback_url": "https://example.com/webhooks/soroban-pulse",
    "from_ledger": 1234000,
    "subscription_type": "webhook",
    "batch_size": 25,
    "batch_timeout_ms": 5000
  }'
```

Response includes the subscription `id`. Acknowledge processed ledgers so the
subscription doesn't redeliver them:

```bash
curl -X POST http://localhost:3000/subscriptions/<id>/ack \
  -H "Content-Type: application/json" \
  -d '{"ledger": 1234050}'
```

Pause/resume without deleting the subscription:

```bash
curl -X POST http://localhost:3000/subscriptions/<id>/pause
curl -X POST http://localhost:3000/subscriptions/<id>/resume
```

## 2. Paginate through historical events for a contract

`GET /v1/events/contract/{contract_id}` supports both offset (`page`/`limit`)
and cursor-based pagination. Prefer the cursor for large backfills — it's
stable under concurrent writes, offset pagination isn't.

```bash
CONTRACT="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4"
CURSOR=""

while true; do
  URL="http://localhost:3000/v1/events/contract/${CONTRACT}?limit=100"
  [ -n "$CURSOR" ] && URL="${URL}&cursor=${CURSOR}"

  RESP=$(curl -s "$URL")
  echo "$RESP" | jq -c '.data[]'

  CURSOR=$(echo "$RESP" | jq -r '.next_cursor // empty')
  [ -z "$CURSOR" ] && break
done
```

## 3. Verify a webhook signature

Every webhook delivery is signed with HMAC-SHA256 in the `X-Signature-256`
header (`sha256=<hex_digest>` over the raw request body). Full algorithm
detail and language examples (Rust, Python, Node) live in
[`docs/webhook_signing.md`](webhook_signing.md) — this is the short version:

```python
import hmac
import hashlib

def verify_signature(secret: str, raw_body: bytes, header_value: str) -> bool:
    algo, _, provided_sig = header_value.partition("=")
    if algo != "sha256":
        return False
    expected = hmac.new(secret.encode(), raw_body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, provided_sig)
```

Reject the request (do not process the payload) if verification fails.

## 4. Handle rate limiting with backoff

The default limit is 60 requests/IP/minute (`RATE_LIMIT_PER_MINUTE`). A `429`
response includes `Retry-After` and `X-RateLimit-*` headers:

```python
import time
import requests

def get_events(base_url: str, **params):
    while True:
        resp = requests.get(f"{base_url}/v1/events", params=params)
        if resp.status_code == 429:
            time.sleep(int(resp.headers.get("Retry-After", 5)))
            continue
        resp.raise_for_status()
        return resp.json()
```

The Go, Python, and TypeScript SDKs already implement this pattern
internally — see [`docs/client-libraries.md`](client-libraries.md#retry-and-backoff-behavior)
rather than reimplementing it if you're using one of them. Full recipe detail
(shell + Python variants, retryable-vs-not table) is in
[`docs/api-usage.md`](api-usage.md#rate-limiting).

## 5. Stream live events for one contract via SSE

`GET /v1/events/contract/{contract_id}/stream` is the preferred SSE endpoint
for a single contract (`/v1/events/stream` also exists but is documented as
less preferred for this case).

```bash
curl -N "http://localhost:3000/v1/events/contract/CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4/stream"
```

```javascript
const es = new EventSource(
  "http://localhost:3000/v1/events/contract/CAAA.../stream"
);
es.onmessage = (e) => console.log("new event:", JSON.parse(e.data));
es.addEventListener("ping", () => {/* keep-alive, ignore */});
```

Reconnection semantics (`Last-Event-ID` replay) are covered in
[`docs/api-usage.md`](api-usage.md#server-sent-events-sse) and
[`docs/sse-reconnection.md`](sse-reconnection.md).

## 6. Bulk-export events as NDJSON

`GET /v1/events/export` supports `csv`, `parquet`, and `jsonl` output, plus
the same filters as `/v1/events`:

```bash
curl -H "Authorization: Bearer $API_KEY" \
  "http://localhost:3000/v1/events/export?format=jsonl&event_type=contract&from_ledger=1000000" \
  > events.ndjson
```

For CSV with renamed columns, pass `field_map` as a JSON string:

```bash
curl -H "Authorization: Bearer $API_KEY" \
  "http://localhost:3000/v1/events/export?format=csv&field_map=%7B%22contract_id%22%3A%22contract%22%7D" \
  > events.csv
```

## 7. Look up events for multiple transactions in one call

`POST /v1/events/tx/batch` avoids N round-trips when you already have a list
of transaction hashes (e.g., collected from your own ledger watcher):

```bash
curl -X POST http://localhost:3000/v1/events/tx/batch \
  -H "Content-Type: application/json" \
  -d '{
    "hashes": [
      "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b",
      "2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c"
    ]
  }' | jq
```

Response is a map of `tx_hash -> events[]`; hashes with no indexed events
come back as an empty array rather than being omitted. Max 100 hashes per
call — a larger batch returns `400`.

## 8. Trace related events across a transaction chain

`GET /v1/events/tx/{tx_hash}/related` follows references inside `event_data`
to pull in events from other transactions the root transaction points to
(e.g., cross-contract calls) — useful for reconstructing a full call graph
without walking it yourself.

```bash
curl "http://localhost:3000/v1/events/tx/1a2b3c4d.../related?depth=2" | jq '.data[] | {tx_hash, contract_id, ledger}'
```

`depth` defaults to 1 and is capped at 3 server-side. For genuinely
cross-chain traces (not just cross-contract), see
[`docs/cross_chain_correlation.md`](cross_chain_correlation.md).
